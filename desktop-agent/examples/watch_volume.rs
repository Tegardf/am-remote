//! Probe (read-only): prints Apple Music's Windows per-app volume (Core Audio, what the agent's
//! `/volume` controls) every time it changes. Move Apple Music's own slider and the Windows
//! volume mixer slider and see which one moves this value. Stop with Ctrl+C.
//! `cargo run --example watch_volume`

use std::thread::sleep;
use std::time::Duration;

use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

use am_remote_agent::media::audio;

fn main() -> windows::core::Result<()> {
    // SAFETY: first COM call on the main thread.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    println!("Watching Apple Music's Windows per-app volume. Ctrl+C to stop.");
    let mut volume = audio::AppleMusicVolume::default();
    let mut last = None;
    loop {
        let now = volume.get()?;
        if now != last {
            match now {
                Some(v) => println!("per-app volume: {}% muted={}", v.level, v.muted),
                None => println!("no Apple Music audio session"),
            }
            last = now;
        }
        sleep(Duration::from_millis(300));
    }
}
