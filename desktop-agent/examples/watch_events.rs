//! Probe (read-only): starts the real Windows backend, prints the current state and artwork, then
//! every live event for a while. Skip a track, pause, or change the volume in the Windows mixer
//! while it runs. `cargo run --example watch_events -- [seconds]` (default 10)

use std::time::Duration;

use am_remote_agent::media::{MediaBackend, WindowsBackend};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(10);
    let backend = WindowsBackend::start()?;
    let mut events = backend.subscribe();

    println!("running: {}", backend.apple_music_running().await);
    println!("state  : {:?}", backend.state().await);
    match backend.artwork().await {
        Some(art) => println!("artwork: {} bytes, {}", art.bytes.len(), art.content_type),
        None => println!("artwork: none"),
    }

    println!("--- events for {seconds} s ---");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    while let Ok(Ok(event)) = tokio::time::timeout_at(deadline, events.recv()).await {
        println!("event  : {event:?}");
    }
    println!("--- done ---");
    Ok(())
}
