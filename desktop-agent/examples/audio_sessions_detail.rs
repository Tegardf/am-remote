//! Probe (read-only): every Core Audio session on the default render device with its display
//! name, state, volume and live peak level (sampled for 2 s). The session with a peak above 0
//! while Apple Music plays is the audible one. Run while a song is playing:
//! `cargo run --example audio_sessions_detail`

use std::thread::sleep;
use std::time::Duration;

use windows::core::{Interface, PWSTR};
use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
    ISimpleAudioVolume, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
};

use am_remote_agent::media::audio::process_name;

/// Reads and frees a COM-allocated wide string.
fn text(p: windows::core::Result<PWSTR>) -> String {
    match p {
        Ok(p) if !p.is_null() => {
            // SAFETY: the string came from COM and is freed exactly once, after copying.
            unsafe {
                let s = p.to_string().unwrap_or_default();
                CoTaskMemFree(Some(p.0.cast()));
                s
            }
        }
        _ => String::new(),
    }
}

fn main() -> windows::core::Result<()> {
    // SAFETY: first COM call on the main thread; plain COM calls below.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let manager = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None)?;
        let sessions = manager.GetSessionEnumerator()?;
        let count = sessions.GetCount()?;

        let mut rows = Vec::new();
        for i in 0..count {
            let control = sessions.GetSession(i)?;
            let c2 = control.cast::<IAudioSessionControl2>()?;
            let pid = c2.GetProcessId()?;
            let volume = control.cast::<ISimpleAudioVolume>()?;
            let meter = control.cast::<IAudioMeterInformation>()?;
            rows.push((i, pid, control, c2, volume, meter));
        }

        // Sample the meters for 2 s and keep the highest peak per session.
        let mut peaks = vec![0f32; rows.len()];
        for _ in 0..20 {
            for (n, row) in rows.iter().enumerate() {
                peaks[n] = peaks[n].max(row.5.GetPeakValue().unwrap_or(0.0));
            }
            sleep(Duration::from_millis(100));
        }

        println!("{count} session(s) on the default render device");
        for (n, (i, pid, control, c2, volume, _)) in rows.iter().enumerate() {
            println!(
                "#{i} pid {pid} {}",
                process_name(*pid).unwrap_or_else(|| "(unknown)".into())
            );
            println!("    display name : {:?}", text(control.GetDisplayName()));
            println!("    session id   : {}", text(c2.GetSessionIdentifier()));
            println!(
                "    instance id  : {}",
                text(c2.GetSessionInstanceIdentifier())
            );
            println!(
                "    state        : {:?}  system-sounds: {}",
                control.GetState()?,
                c2.IsSystemSoundsSession().is_ok()
            );
            println!(
                "    volume       : {:.0}% muted={}   peak over 2 s: {:.3}",
                volume.GetMasterVolume()? * 100.0,
                volume.GetMute()?.as_bool(),
                peaks[n]
            );
        }
    }
    Ok(())
}
