//! Spike probe: does opening a music.apple.com link (in several forms) make the Apple Music app for
//! Windows open it AND start playing, without anyone clicking?
//!
//! THIS STARTS MUSIC ON THIS LAPTOP. Apple Music must be running and signed in.
//!
//!   cargo run --example spike_link_playback -- [--all] [--include-https] "label|https-url|expected title" ...
//!
//! For each URL it tries the link forms in order and stops at the first one that works (`--all` tries
//! every form). Per trial it pauses, records the SMTC state, launches the link with public OS means
//! (`rundll32 url.dll,FileProtocolHandler`, which is ShellExecute, or the app's own `AppleMusic.exe
//! /url` execution alias), polls SMTC for up to 10 s and prints one JSON line:
//! `{ url, form, app_opened, track_changed, playing, elapsed_ms, title }`. It pauses at the end.
//! The plain https form is only tried with `--include-https` (it may open a browser tab).

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use am_remote_agent::media::{MediaBackend, PlaybackStatus, Transport, WindowsBackend};
use serde_json::json;

const POLL: Duration = Duration::from_millis(400);
const WAIT: Duration = Duration::from_secs(10);

enum Launch {
    /// ShellExecute through `rundll32 url.dll,FileProtocolHandler <url>`.
    Shell(String),
    /// The app's own execution alias: `AppleMusic.exe /url <url>`.
    Cli(String),
}

fn with_scheme(url: &str, scheme: &str) -> String {
    url.replacen("https://", &format!("{scheme}://"), 1)
}

fn forms(url: &str, include_https: bool) -> Vec<(&'static str, Launch)> {
    let mut forms = vec![
        ("shell music://", Launch::Shell(with_scheme(url, "music"))),
        ("shell musics://", Launch::Shell(with_scheme(url, "musics"))),
        ("shell itmss://", Launch::Shell(with_scheme(url, "itmss"))),
        ("cli /url https://", Launch::Cli(url.to_owned())),
        ("cli /url music://", Launch::Cli(with_scheme(url, "music"))),
    ];
    if include_https {
        forms.push(("shell https://", Launch::Shell(url.to_owned())));
    }
    forms
}

fn launch(how: &Launch) -> bool {
    let status = match how {
        Launch::Shell(url) => Command::new("rundll32.exe")
            .arg("url.dll,FileProtocolHandler")
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
        Launch::Cli(url) => Command::new("AppleMusic.exe")
            .arg("/url")
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .and_then(|mut child| {
                // The alias hands the link to the running app and exits; never wait forever.
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    if let Some(status) = child.try_wait()? {
                        return Ok(status);
                    }
                    if Instant::now() > deadline {
                        let _ = child.kill();
                        return child.wait();
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }),
    };
    status.is_ok()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut all = false;
    let mut include_https = false;
    let mut targets = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--all" => all = true,
            "--include-https" => include_https = true,
            _ => targets.push(arg),
        }
    }
    let backend = WindowsBackend::start()?;
    if !backend.apple_music_running().await {
        println!("Apple Music has no media session: open it and play a song once, then run again.");
        return Ok(());
    }

    for target in targets {
        let mut parts = target.splitn(3, '|');
        let label = parts.next().unwrap_or("?");
        let url = parts.next().unwrap_or_default().to_owned();
        let expected = parts.next().unwrap_or_default().to_lowercase();
        println!("== {label}: {url}");

        for (form, how) in forms(&url, include_https) {
            // Start from a quiet, known state.
            let _ = backend.transport(Transport::Pause).await;
            tokio::time::sleep(Duration::from_millis(800)).await;
            let before = backend.state().await;

            let started = Instant::now();
            let app_opened = launch(&how);
            let (mut track_changed, mut playing, mut elapsed_ms, mut title) =
                (false, false, None, before.title.clone());
            while started.elapsed() < WAIT {
                tokio::time::sleep(POLL).await;
                let now = backend.state().await;
                title = now.title.clone();
                track_changed = now.title != before.title;
                playing = now.status == PlaybackStatus::Playing;
                if track_changed && playing {
                    elapsed_ms = Some(started.elapsed().as_millis() as u64);
                    break;
                }
            }
            let matches_expected = !expected.is_empty() && title.to_lowercase().contains(&expected);
            println!(
                "{}",
                json!({
                    "url": url, "form": form, "app_opened": app_opened,
                    "track_changed": track_changed, "playing": playing,
                    "elapsed_ms": elapsed_ms, "title": title,
                    "before_title": before.title, "matches_expected_title": matches_expected
                })
            );
            let _ = backend.transport(Transport::Pause).await;
            if track_changed && playing && !all {
                break;
            }
        }
    }
    println!("== done (paused)");
    Ok(())
}
