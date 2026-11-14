//! Probe: starts the real Windows backend and prints what it reads. Read-only: it never
//! changes playback or volume. Run with Apple Music open: `cargo run --example read_state`.

use am_remote_agent::media::{MediaBackend, WindowsBackend};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let backend = WindowsBackend::start()?;
    println!(
        "apple_music_running: {}",
        backend.apple_music_running().await
    );
    match backend.volume().await {
        Ok(volume) => println!("volume: {}% muted={}", volume.level, volume.muted),
        Err(e) => println!("volume: error ({e})"),
    }
    Ok(())
}
