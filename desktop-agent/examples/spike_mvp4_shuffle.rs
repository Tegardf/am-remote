//! MVP 4 spike, A1/A2: what SMTC reports for shuffle and repeat, and (only with `--change`) whether
//! SMTC can change them. The original state is restored at the end.
//!   cargo run --example spike_mvp4_shuffle              (read only)
//!   cargo run --example spike_mvp4_shuffle -- --change  (toggles shuffle / repeat, then restores)

use std::time::Duration;

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
};
use windows::Media::MediaPlaybackAutoRepeatMode;

fn repeat_name(mode: Option<MediaPlaybackAutoRepeatMode>) -> String {
    match mode {
        None => "not reported".into(),
        Some(m) if m == MediaPlaybackAutoRepeatMode::None => "None".into(),
        Some(m) if m == MediaPlaybackAutoRepeatMode::List => "List".into(),
        Some(m) if m == MediaPlaybackAutoRepeatMode::Track => "Track".into(),
        Some(m) => format!("other({})", m.0),
    }
}

fn read(session: &GlobalSystemMediaTransportControlsSession) -> windows::core::Result<()> {
    let info = session.GetPlaybackInfo()?;
    let controls = info.Controls()?;
    let shuffle = info.IsShuffleActive().ok().and_then(|r| r.Value().ok());
    let repeat = info.AutoRepeatMode().ok().and_then(|r| r.Value().ok());
    println!(
        "  IsShuffleActive={shuffle:?}  AutoRepeatMode={}  status={:?}",
        repeat_name(repeat),
        info.PlaybackStatus().map(|s| s.0)
    );
    println!(
        "  Controls: IsShuffleEnabled={:?} IsRepeatEnabled={:?} IsNextEnabled={:?} IsPlayPauseToggleEnabled={:?}",
        controls.IsShuffleEnabled(),
        controls.IsRepeatEnabled(),
        controls.IsNextEnabled(),
        controls.IsPlayPauseToggleEnabled()
    );
    Ok(())
}

fn main() -> windows::core::Result<()> {
    let change = std::env::args().any(|a| a == "--change");
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()?;
    let sessions = manager.GetSessions()?;
    let mut apple = None;
    for session in sessions {
        let aumid = session.SourceAppUserModelId()?.to_string();
        println!("session: {aumid}");
        if aumid.to_lowercase().starts_with("appleinc.applemusicwin") {
            apple = Some(session);
        }
    }
    let session = apple.expect("Apple Music has no SMTC session: play something first");
    println!("Apple Music session, as it is now:");
    read(&session)?;
    if !change {
        return Ok(());
    }

    let info = session.GetPlaybackInfo()?;
    let was_shuffle = info.IsShuffleActive().ok().and_then(|r| r.Value().ok());
    let was_repeat = info.AutoRepeatMode().ok().and_then(|r| r.Value().ok());

    for target in [true, false] {
        let accepted = session.TryChangeShuffleActiveAsync(target)?.join();
        std::thread::sleep(Duration::from_millis(1500));
        println!("TryChangeShuffleActiveAsync({target}) -> {accepted:?}");
        read(&session)?;
    }
    for target in [
        MediaPlaybackAutoRepeatMode::List,
        MediaPlaybackAutoRepeatMode::Track,
        MediaPlaybackAutoRepeatMode::None,
    ] {
        let accepted = session.TryChangeAutoRepeatModeAsync(target)?.join();
        std::thread::sleep(Duration::from_millis(1500));
        println!(
            "TryChangeAutoRepeatModeAsync({}) -> {accepted:?}",
            repeat_name(Some(target))
        );
        read(&session)?;
    }

    // Restore what the user had.
    if let Some(shuffle) = was_shuffle {
        let _ = session.TryChangeShuffleActiveAsync(shuffle)?.join();
    }
    if let Some(repeat) = was_repeat {
        let _ = session.TryChangeAutoRepeatModeAsync(repeat)?.join();
    }
    std::thread::sleep(Duration::from_millis(1500));
    println!(
        "restored to shuffle={was_shuffle:?} repeat={}:",
        repeat_name(was_repeat)
    );
    read(&session)?;
    Ok(())
}
