//! Validating and parsing Apple Music links for `POST /play/track`.
//!
//! Only `https://music.apple.com/...` is accepted: the link is handed to the Apple Music app, so a
//! look-alike host must never get through. The app is asked to open the same link with the
//! `music://` scheme, because that is the form the app registered and navigates on (verified in
//! `docs/spikes/link-playback.md`).

use std::fmt;

use url::Url;

const HOST: &str = "music.apple.com";
const MAX_LEN: usize = 2048;

/// A validated `https://music.apple.com/...` link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppleLink {
    /// Two-letter storefront from the path (`/id/album/...`), if there is one.
    pub storefront: Option<String>,
    /// The track id from the `i` query parameter (digits only), if there is one.
    pub track_id: Option<String>,
    /// The same link with the `music://` scheme: what the Apple Music app is asked to open.
    pub open_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkError {
    TooLong,
    NotAUrl,
    NotHttps,
    WrongHost,
    /// Credentials or a port in the URL: never legitimate for this host.
    Suspicious,
    /// `i` is present but is not a number.
    BadTrackId,
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooLong => "url is too long",
            Self::NotAUrl => "url is not a valid URL",
            Self::NotHttps => "url must start with https://music.apple.com/",
            Self::WrongHost => "url must be on music.apple.com",
            Self::Suspicious => "url must not contain credentials or a port",
            Self::BadTrackId => "the track id in the url (i=...) must be a number",
        })
    }
}

impl std::error::Error for LinkError {}

/// Accepts `https://music.apple.com/...` only.
pub fn parse_apple_music_url(input: &str) -> Result<AppleLink, LinkError> {
    if input.len() > MAX_LEN {
        return Err(LinkError::TooLong);
    }
    let url = Url::parse(input.trim()).map_err(|_| LinkError::NotAUrl)?;
    if url.scheme() != "https" {
        return Err(LinkError::NotHttps);
    }
    if url.host_str() != Some(HOST) {
        return Err(LinkError::WrongHost);
    }
    if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
        return Err(LinkError::Suspicious);
    }

    let storefront = url
        .path_segments()
        .and_then(|mut segments| segments.next())
        .filter(|s| s.len() == 2 && s.chars().all(|c| c.is_ascii_alphabetic()))
        .map(str::to_ascii_lowercase);

    let track_id = match url.query_pairs().find(|(key, _)| key == "i") {
        None => None,
        Some((_, value)) if !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()) => {
            Some(value.into_owned())
        }
        Some(_) => return Err(LinkError::BadTrackId),
    };

    // Rebuilt from the validated parts only: the path and the numeric song id. Any other query
    // parameter a caller adds is not forwarded to the app.
    let query = track_id
        .as_deref()
        .map(|id| format!("?i={id}"))
        .unwrap_or_default();
    Ok(AppleLink {
        storefront,
        open_url: format!("music://{HOST}{}{query}", url.path()),
        track_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SONG: &str = "https://music.apple.com/id/album/jatuh-suka/1612163750?i=1612164263&uo=4";

    #[test]
    fn a_song_link_is_parsed() {
        let link = parse_apple_music_url(SONG).unwrap();
        assert_eq!(link.storefront.as_deref(), Some("id"));
        assert_eq!(link.track_id.as_deref(), Some("1612164263"));
        assert_eq!(
            link.open_url,
            "music://music.apple.com/id/album/jatuh-suka/1612163750?i=1612164263"
        );
    }

    #[test]
    fn links_without_a_track_or_storefront_are_still_valid_links() {
        let album =
            parse_apple_music_url("https://music.apple.com/us/album/manusia/1612163750").unwrap();
        assert_eq!(album.track_id, None);
        assert_eq!(album.storefront.as_deref(), Some("us"));
        let bare = parse_apple_music_url("https://music.apple.com/").unwrap();
        assert_eq!(bare.storefront, None);
        assert_eq!(bare.open_url, "music://music.apple.com/");
    }

    #[test]
    fn surrounding_whitespace_and_host_case_are_tolerated() {
        let link = parse_apple_music_url("  HTTPS://Music.Apple.Com/id/album/x/1?i=2 ").unwrap();
        assert_eq!(link.open_url, "music://music.apple.com/id/album/x/1?i=2");
    }

    #[test]
    fn other_hosts_are_rejected() {
        for bad in [
            "https://evil.com/id/album/x/1?i=2",
            "https://music.apple.com.evil.com/id/album/x/1?i=2",
            "https://evilmusic.apple.com/id/album/x/1?i=2",
            "https://apple.com/id/album/x/1?i=2",
            "https://evil.com/music.apple.com/id/album/x/1?i=2",
            "https://evil.com/?u=https://music.apple.com/",
            "https://itunes.apple.com/id/album/x/1?i=2",
        ] {
            assert_eq!(
                parse_apple_music_url(bad),
                Err(LinkError::WrongHost),
                "{bad}"
            );
        }
    }

    #[test]
    fn other_schemes_are_rejected() {
        for bad in [
            "http://music.apple.com/id/album/x/1?i=2",
            "music://music.apple.com/id/album/x/1?i=2",
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
            "ftp://music.apple.com/x",
        ] {
            assert!(parse_apple_music_url(bad).is_err(), "{bad}");
        }
        assert_eq!(
            parse_apple_music_url("http://music.apple.com/x"),
            Err(LinkError::NotHttps)
        );
    }

    #[test]
    fn credentials_and_ports_are_rejected() {
        assert_eq!(
            parse_apple_music_url("https://user:pw@music.apple.com/x"),
            Err(LinkError::Suspicious)
        );
        assert_eq!(
            parse_apple_music_url("https://music.apple.com:8443/x"),
            Err(LinkError::Suspicious)
        );
    }

    #[test]
    fn garbage_is_rejected() {
        for bad in ["", "   ", "not a url", "music.apple.com/id/album/x/1"] {
            assert_eq!(
                parse_apple_music_url(bad),
                Err(LinkError::NotAUrl),
                "{bad:?}"
            );
        }
        let long = format!("https://music.apple.com/{}", "a".repeat(3000));
        assert_eq!(parse_apple_music_url(&long), Err(LinkError::TooLong));
    }

    #[test]
    fn the_track_id_must_be_a_number() {
        for bad in [
            "https://music.apple.com/id/album/x/1?i=abc",
            "https://music.apple.com/id/album/x/1?i=",
            "https://music.apple.com/id/album/x/1?i=12%2034",
            "https://music.apple.com/id/album/x/1?i=1;calc",
        ] {
            assert_eq!(
                parse_apple_music_url(bad),
                Err(LinkError::BadTrackId),
                "{bad}"
            );
        }
    }

    #[test]
    fn only_the_song_id_is_forwarded_from_the_query() {
        let link = parse_apple_music_url(
            "https://music.apple.com/id/album/x/1?foo=bar&i=2&uo=4&cmd=calc%20%26%20dir",
        )
        .unwrap();
        assert_eq!(link.open_url, "music://music.apple.com/id/album/x/1?i=2");
        let no_song =
            parse_apple_music_url("https://music.apple.com/id/album/x/1?foo=bar").unwrap();
        assert_eq!(no_song.open_url, "music://music.apple.com/id/album/x/1");
    }

    #[test]
    fn a_fragment_is_dropped_from_the_link_the_app_opens() {
        let link = parse_apple_music_url("https://music.apple.com/id/album/x/1?i=2#frag").unwrap();
        assert_eq!(link.open_url, "music://music.apple.com/id/album/x/1?i=2");
    }
}
