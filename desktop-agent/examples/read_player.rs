//! Probe (read-only): prints what SMTC reports for the Apple Music session: metadata, timeline,
//! playback status, which controls are enabled, and the artwork's content type and size.
//! Run with Apple Music playing: `cargo run --example read_player`

use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;
use windows::Storage::Streams::DataReader;

fn main() -> windows::core::Result<()> {
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()?;
    for session in manager.GetSessions()? {
        let aumid = session.SourceAppUserModelId()?.to_string();
        if !aumid.to_lowercase().starts_with("appleinc.applemusicwin") {
            continue;
        }
        println!("session: {aumid}");

        let props = session.TryGetMediaPropertiesAsync()?.join()?;
        println!("title : {:?}", props.Title()?.to_string());
        println!("artist: {:?}", props.Artist()?.to_string());
        println!("album : {:?}", props.AlbumTitle()?.to_string());

        let info = session.GetPlaybackInfo()?;
        println!(
            "status: {:?} (raw {})",
            info.PlaybackStatus()?,
            info.PlaybackStatus()?.0
        );
        let controls = info.Controls()?;
        println!(
            "controls: play={} pause={} next={} prev={} position(seek)={}",
            controls.IsPlayEnabled()?,
            controls.IsPauseEnabled()?,
            controls.IsNextEnabled()?,
            controls.IsPreviousEnabled()?,
            controls.IsPlaybackPositionEnabled()?
        );

        let timeline = session.GetTimelineProperties()?;
        println!(
            "timeline ticks(100ns): start={} end={} position={} last_updated(univ)={}",
            timeline.StartTime()?.Duration,
            timeline.EndTime()?.Duration,
            timeline.Position()?.Duration,
            timeline.LastUpdatedTime()?.UniversalTime
        );

        match props.Thumbnail() {
            Ok(reference) => {
                let stream = reference.OpenReadAsync()?.join()?;
                let size = stream.Size()? as u32;
                println!(
                    "thumbnail: content_type={:?} size={size} bytes",
                    stream.ContentType()?.to_string()
                );
                let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
                reader.LoadAsync(size)?.join()?;
                let mut bytes = vec![0u8; size as usize];
                reader.ReadBytes(&mut bytes)?;
                println!("first bytes: {:02x?}", &bytes[..bytes.len().min(8)]);
            }
            Err(e) => println!("thumbnail: none ({e})"),
        }
    }
    Ok(())
}
