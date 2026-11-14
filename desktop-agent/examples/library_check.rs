//! Probe: exercises the real MVP 3 code (search, playlists, playing a song or a playlist) against the
//! running Apple Music, through the same backend the agent uses.
//!
//!   cargo run --example library_check -- list                list the library playlists (read-only)
//!   cargo run --example library_check -- search <text>       search the catalog (read-only)
//!   cargo run --example library_check -- track <https-url>   play that song (STARTS MUSIC)
//!   cargo run --example library_check -- playlist <index>    play the Nth listed playlist (STARTS MUSIC)
//!
//! Playing commands pause again after a few seconds.

use std::time::{Duration, Instant};

use am_remote_agent::media::links::parse_apple_music_url;
use am_remote_agent::media::{MediaBackend, PlayTrack, Transport, WindowsBackend};
use am_remote_agent::search::{system_storefront, ItunesSearch, SearchBackend};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .init();
    let mut args = std::env::args().skip(1);
    let action = args.next().unwrap_or_else(|| "list".into());
    let argument = args.next().unwrap_or_default();

    let backend = WindowsBackend::start()?;
    let search = ItunesSearch::new(system_storefront())?;

    match action.as_str() {
        "list" => {
            let started = Instant::now();
            let playlists = backend.playlists().await;
            println!("took {} ms", started.elapsed().as_millis());
            match playlists {
                Ok(list) => {
                    println!("{} playlists", list.len());
                    for (i, p) in list.iter().enumerate() {
                        println!("  #{i} name={:?} id={}", p.name, p.id);
                    }
                }
                Err(e) => println!("error: {e:?}"),
            }
        }
        "search" => {
            for r in search.search(&argument, 5).await? {
                println!("{} | {} | {} | {}", r.title, r.artist, r.album, r.url);
            }
        }
        "track" => {
            let link = parse_apple_music_url(&argument)?;
            let id = link.track_id.clone().expect("the link needs ?i=<song id>");
            let info = search
                .lookup_track(&id, link.storefront.as_deref())
                .await?
                .expect("unknown song");
            println!(
                "playing {:?} by {:?} on {:?}",
                info.title, info.artist, info.album
            );
            let started = Instant::now();
            let result = backend
                .play_track(PlayTrack {
                    open_url: link.open_url,
                    title: info.title,
                    artist: info.artist,
                    album: info.album,
                })
                .await;
            println!(
                "result: {result:?} after {} ms",
                started.elapsed().as_millis()
            );
            let now = backend.state().await;
            println!(
                "now playing: {:?} by {:?} ({:?})",
                now.title, now.artist, now.status
            );
            tokio::time::sleep(Duration::from_secs(4)).await;
            backend.transport(Transport::Pause).await.ok();
        }
        "unknown" => {
            let started = Instant::now();
            let result = backend
                .play_playlist("DBID:0x0-PID:0x0-PPID:0x0-PIPID:0x0-IKIND:ePlaylist")
                .await;
            println!(
                "unknown id -> {result:?} after {} ms",
                started.elapsed().as_millis()
            );
        }
        "playlist" => {
            let index: usize = argument.parse().unwrap_or(0);
            let list = backend.playlists().await?;
            let target = list.get(index).expect("no such playlist");
            println!("playing playlist #{index} {:?}", target.name);
            let started = Instant::now();
            let result = backend.play_playlist(&target.id).await;
            println!(
                "result: {result:?} after {} ms",
                started.elapsed().as_millis()
            );
            let now = backend.state().await;
            println!(
                "now playing: {:?} by {:?} ({:?})",
                now.title, now.artist, now.status
            );
            tokio::time::sleep(Duration::from_secs(4)).await;
            backend.transport(Transport::Pause).await.ok();
        }
        other => println!("unknown action {other}"),
    }
    Ok(())
}
