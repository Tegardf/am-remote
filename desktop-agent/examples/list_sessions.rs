//! Probe: prints every SMTC (media session) source AUMID on this machine.
//! Run with Apple Music open and having played something: `cargo run --example list_sessions`.

use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

fn main() -> windows::core::Result<()> {
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()?;
    let sessions = manager.GetSessions()?;
    println!("{} SMTC session(s)", sessions.Size()?);
    for session in sessions {
        let status = session.GetPlaybackInfo()?.PlaybackStatus()?;
        println!(
            "  AUMID: {}  (playback status: {status:?})",
            session.SourceAppUserModelId()?
        );
    }
    Ok(())
}
