//! Probe: sets Apple Music's volume (through the same code the agent uses) to a test level for a
//! few seconds, then restores the original level and mute state. You will hear it get quieter.
//! `cargo run --example volume_check -- [level] [seconds]`   (defaults: 60 for 6 s)

use std::thread::sleep;
use std::time::Duration;

use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

use am_remote_agent::media::audio::AppleMusicVolume;

fn main() -> windows::core::Result<()> {
    let mut args = std::env::args().skip(1);
    let level: u8 = args.next().and_then(|a| a.parse().ok()).unwrap_or(60);
    let seconds: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(6);

    // SAFETY: first COM call on the main thread.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let mut volume = AppleMusicVolume::default();
    let Some(original) = volume.get()? else {
        println!("Apple Music has no audio session; play something first");
        return Ok(());
    };
    println!("before : {original:?}");

    let result = volume.set(Some(level), Some(false));
    println!("set {level}: {result:?}");
    sleep(Duration::from_secs(seconds));

    // Always restore, even if setting failed halfway.
    let restored = volume.set(Some(original.level), Some(original.muted));
    println!("restored: {restored:?} (original was {original:?})");
    result.map(|_| ())
}
