//! Probe: prints every Core Audio session on the default render device with its process name.
//! Run with Apple Music open and having played something: `cargo run --example list_audio_sessions`.

use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

use am_remote_agent::media::audio;

fn main() -> windows::core::Result<()> {
    // SAFETY: called once, first thing on the main thread.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let sessions = audio::list_sessions()?;
    println!("{} audio session(s)", sessions.len());
    for session in &sessions {
        let name = session.process_name.as_deref().unwrap_or("(unknown)");
        println!("  pid {:>6}  {name}", session.pid);
    }
    match audio::AppleMusicVolume::default().get()? {
        Some(v) => println!("Apple Music volume: {}% muted={}", v.level, v.muted),
        None => println!("Apple Music has no audio session (play something first)"),
    }
    Ok(())
}
