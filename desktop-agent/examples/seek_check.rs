//! Probe: does `TryChangePlaybackPositionAsync` really move Apple Music although
//! `IsPlaybackPositionEnabled` is false? Jumps 3 s forward, checks the position, jumps back.
//! Run with Apple Music playing (not in the last 10 s of a song): `cargo run --example seek_check`

use std::thread::sleep;
use std::time::Duration;

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
};

const TICKS_PER_SEC: i64 = 10_000_000;
/// Ticks between 1601-01-01 and 1970-01-01.
const EPOCH_DIFF_TICKS: i64 = 116_444_736_000_000_000;

/// Position in seconds as the app shows it now: reported position + time since it was reported.
fn effective_secs(
    session: &GlobalSystemMediaTransportControlsSession,
) -> windows::core::Result<f64> {
    let timeline = session.GetTimelineProperties()?;
    let now_ticks = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64 / 100)
        .unwrap_or(0)
        + EPOCH_DIFF_TICKS;
    let updated = timeline.LastUpdatedTime()?.UniversalTime;
    Ok((timeline.Position()?.Duration + (now_ticks - updated)) as f64 / TICKS_PER_SEC as f64)
}

fn main() -> windows::core::Result<()> {
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()?;
    for session in manager.GetSessions()? {
        if !session
            .SourceAppUserModelId()?
            .to_string()
            .to_lowercase()
            .starts_with("appleinc.applemusicwin")
        {
            continue;
        }
        let end = session.GetTimelineProperties()?.EndTime()?.Duration;
        let start = effective_secs(&session)?;
        println!("position now: {start:.1}s");
        let target = ((start + 3.0) * TICKS_PER_SEC as f64) as i64;
        if target > end - 10 * TICKS_PER_SEC {
            println!("too close to the end of the song; run it earlier in a song");
            return Ok(());
        }
        let accepted = session.TryChangePlaybackPositionAsync(target)?.join()?;
        sleep(Duration::from_millis(1000));
        let after = effective_secs(&session)?;
        println!("seek +3 s accepted={accepted}; position 1 s later: {after:.1}s (expected about {:.1}s if it worked, {:.1}s if ignored)", start + 4.0, start + 1.0);
        // Jump back so the listener loses nothing.
        let back = ((after - 3.0) * TICKS_PER_SEC as f64) as i64;
        session.TryChangePlaybackPositionAsync(back)?.join()?;
        println!("jumped back");
    }
    Ok(())
}
