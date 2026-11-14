//! Per-app volume through Windows Core Audio.
//!
//! Default render device -> `IAudioSessionManager2` -> the sessions of the Apple Music package
//! -> `ISimpleAudioVolume`. Core Audio's 0.0-1.0 scalar never leaves this module: callers see 0-100.
//!
//! **Which session is audible?** Apple Music's window process (`AppleMusic.exe`) has a session that
//! stays *inactive*; the sound is rendered by `AMPLibraryAgent.exe` from the same package, whose
//! session is the *active* one. The Windows mixer therefore shows two "Apple Music" entries and only
//! one of them changes the loudness. So we match by package, change all of the package's sessions
//! together, and read the level from the active one.
//!
//! All functions need COM initialised on the calling thread (the media thread does that).

use serde::{Deserialize, Serialize};
use windows::core::{Interface, Result, PWSTR};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::{
    eConsole, eRender, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2,
    IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// Executable of Apple Music's main window (Microsoft Store version). Used to find its window for
/// UI Automation probes; volume does NOT use it (see the module docs).
// verified: `tasklist` on the dev laptop shows `AppleMusic.exe` while the app runs, 2026-11-14
pub const APPLE_MUSIC_PROCESS: &str = "AppleMusic.exe";

/// The Apple Music package folder inside the `WindowsApps` folder. Every process whose executable
/// lives in such a folder belongs to the app. Matched case-insensitively on the full image path,
/// including the `WindowsApps` parent (a folder with the same name anywhere else on the disk is
/// not the app, and its windows must never be acted on).
// verified: `cargo run --example audio_sessions_detail` while a song played: the audible session
// was `...\WindowsApps\AppleInc.AppleMusicWin_1.1792.39942.0_x64__nzyj5cx40ttqa\AMPLibraryAgent.exe`
// (active, peak 0.5) and `AppleMusic.exe` was inactive (peak 0), 2026-11-14
pub const APPLE_MUSIC_PACKAGE_DIR_PREFIX: &str = "\\windowsapps\\appleinc.applemusicwin_";

/// Wire shape of `GET/POST /volume` responses and the `volume` WebSocket event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VolumeState {
    /// 0-100.
    pub level: u8,
    pub muted: bool,
}

/// 0-100 -> 0.0-1.0. Values above 100 are clamped.
pub fn level_to_scalar(level: u8) -> f32 {
    f32::from(level.min(100)) / 100.0
}

/// 0.0-1.0 -> 0-100, rounded to the nearest step and clamped.
pub fn scalar_to_level(scalar: f32) -> u8 {
    (scalar * 100.0).round().clamp(0.0, 100.0) as u8
}

/// True if the executable at `path` is part of the Apple Music package.
pub fn is_apple_music_path(path: &str) -> bool {
    path.replace('/', "\\")
        .to_lowercase()
        .contains(APPLE_MUSIC_PACKAGE_DIR_PREFIX)
}

/// Which of the matching sessions to read the level from: the first active one; if none is active
/// (paused), the one that was last seen active; otherwise the first. `None` if there are none.
fn pick_primary(candidates: &[(bool, &str)], last_active: Option<&str>) -> Option<usize> {
    candidates
        .iter()
        .position(|(active, _)| *active)
        .or_else(|| {
            last_active.and_then(|last| {
                candidates
                    .iter()
                    .position(|(_, instance)| *instance == last)
            })
        })
        .or(if candidates.is_empty() { None } else { Some(0) })
}

/// One audio session on the default render device, for probing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub pid: u32,
    /// File name of the process executable, if it could be read (system sounds have none).
    pub process_name: Option<String>,
}

fn session_manager() -> Result<IAudioSessionManager2> {
    // SAFETY: plain COM calls; COM is initialised on this thread by the caller.
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None)
    }
}

/// Full path of the executable of `pid`, or `None` if it can't be opened.
pub fn process_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    // SAFETY: the handle is closed on every path; the buffer outlives the call.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut length = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(handle);
        result.ok()?;
        Some(String::from_utf16_lossy(&buffer[..length as usize]))
    }
}

/// File name (without directory) of the executable of `pid`, or `None` if it can't be opened.
pub fn process_name(pid: u32) -> Option<String> {
    process_path(pid)?
        .rsplit(['\\', '/'])
        .next()
        .map(str::to_owned)
}

/// Reads a COM-allocated wide string and frees it.
fn take_string(text: PWSTR) -> String {
    if text.is_null() {
        return String::new();
    }
    // SAFETY: the string was allocated by COM and is freed exactly once, after copying.
    unsafe {
        let copy = text.to_string().unwrap_or_default();
        CoTaskMemFree(Some(text.0.cast()));
        copy
    }
}

/// Every audio session on the default render device (used by `examples/list_audio_sessions.rs`).
pub fn list_sessions() -> Result<Vec<SessionInfo>> {
    let mut out = Vec::new();
    // SAFETY: plain COM calls on interfaces obtained above.
    unsafe {
        let sessions = session_manager()?.GetSessionEnumerator()?;
        for index in 0..sessions.GetCount()? {
            let control = sessions
                .GetSession(index)?
                .cast::<IAudioSessionControl2>()?;
            let pid = control.GetProcessId()?;
            out.push(SessionInfo {
                pid,
                process_name: process_name(pid),
            });
        }
    }
    Ok(out)
}

/// A session of the Apple Music package.
struct AppleSession {
    volume: ISimpleAudioVolume,
    active: bool,
    /// Stable id of this session, to remember which one was last audible.
    instance: String,
}

fn apple_sessions() -> Result<Vec<AppleSession>> {
    let mut out = Vec::new();
    // SAFETY: plain COM calls on interfaces obtained above.
    unsafe {
        let sessions = session_manager()?.GetSessionEnumerator()?;
        for index in 0..sessions.GetCount()? {
            // A session that cannot be inspected belongs to some other app: skip it.
            let Ok(control) = sessions.GetSession(index) else {
                continue;
            };
            let Ok(control2) = control.cast::<IAudioSessionControl2>() else {
                continue;
            };
            let Ok(pid) = control2.GetProcessId() else {
                continue;
            };
            if !process_path(pid).is_some_and(|path| is_apple_music_path(&path)) {
                continue;
            }
            out.push(AppleSession {
                volume: control.cast::<ISimpleAudioVolume>()?,
                active: control
                    .GetState()
                    .is_ok_and(|state| state == AudioSessionStateActive),
                instance: control2
                    .GetSessionInstanceIdentifier()
                    .map(take_string)
                    .unwrap_or_default(),
            });
        }
    }
    Ok(out)
}

fn read(volume: &ISimpleAudioVolume) -> Result<VolumeState> {
    // SAFETY: plain COM calls.
    unsafe {
        Ok(VolumeState {
            level: scalar_to_level(volume.GetMasterVolume()?),
            muted: volume.GetMute()?.as_bool(),
        })
    }
}

/// Reads and sets Apple Music's volume. Keep one per media thread: it remembers which session was
/// last audible, so the level can still be read while playback is paused.
#[derive(Debug, Default)]
pub struct AppleMusicVolume {
    last_active: Option<String>,
}

impl AppleMusicVolume {
    /// Index of the session to read from, remembering the active one.
    fn primary(&mut self, sessions: &[AppleSession]) -> Option<usize> {
        let candidates: Vec<(bool, &str)> = sessions
            .iter()
            .map(|s| (s.active, s.instance.as_str()))
            .collect();
        let chosen = pick_primary(&candidates, self.last_active.as_deref())?;
        if sessions[chosen].active {
            self.last_active = Some(sessions[chosen].instance.clone());
        }
        Some(chosen)
    }

    /// Current volume of Apple Music, or `None` if it has no audio session yet
    /// (Windows only creates the session once the app has played something).
    pub fn get(&mut self) -> Result<Option<VolumeState>> {
        let sessions = apple_sessions()?;
        match self.primary(&sessions) {
            Some(index) => read(&sessions[index].volume).map(Some),
            None => Ok(None),
        }
    }

    /// Sets whichever of `level` / `muted` is given on every Apple Music session, then returns the
    /// resulting state. `None` if Apple Music has no audio session.
    pub fn set(&mut self, level: Option<u8>, muted: Option<bool>) -> Result<Option<VolumeState>> {
        let sessions = apple_sessions()?;
        let Some(primary) = self.primary(&sessions) else {
            return Ok(None);
        };
        for session in &sessions {
            // SAFETY: plain COM calls; a null event context means "no originator".
            unsafe {
                if let Some(level) = level {
                    session
                        .volume
                        .SetMasterVolume(level_to_scalar(level), std::ptr::null())?;
                }
                if let Some(muted) = muted {
                    session.volume.SetMute(muted, std::ptr::null())?;
                }
            }
        }
        read(&sessions[primary].volume).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKAGE: &str =
        r"C:\Program Files\WindowsApps\AppleInc.AppleMusicWin_1.1792.39942.0_x64__nzyj5cx40ttqa";

    #[test]
    fn ends_map_exactly() {
        assert_eq!(level_to_scalar(0), 0.0);
        assert_eq!(level_to_scalar(100), 1.0);
        assert_eq!(scalar_to_level(0.0), 0);
        assert_eq!(scalar_to_level(1.0), 100);
    }

    #[test]
    fn scalar_rounds_to_the_nearest_step() {
        assert_eq!(scalar_to_level(0.004), 0);
        assert_eq!(scalar_to_level(0.006), 1);
        assert_eq!(scalar_to_level(0.494), 49);
        assert_eq!(scalar_to_level(0.496), 50);
        assert_eq!(scalar_to_level(0.994), 99);
        assert_eq!(scalar_to_level(0.996), 100);
    }

    #[test]
    fn out_of_range_is_clamped() {
        assert_eq!(scalar_to_level(-0.5), 0);
        assert_eq!(scalar_to_level(1.7), 100);
        assert_eq!(level_to_scalar(250), 1.0);
    }

    #[test]
    fn every_level_survives_a_round_trip() {
        for level in 0..=100u8 {
            assert_eq!(
                scalar_to_level(level_to_scalar(level)),
                level,
                "level {level}"
            );
        }
    }

    #[test]
    fn volume_state_uses_the_contract_field_names() {
        let json = serde_json::to_value(VolumeState {
            level: 42,
            muted: false,
        })
        .unwrap();
        assert_eq!(json, serde_json::json!({ "level": 42, "muted": false }));
    }

    #[test]
    fn both_apple_music_processes_match_by_package() {
        assert!(is_apple_music_path(&format!(
            r"{PACKAGE}\AMPLibraryAgent.exe"
        )));
        assert!(is_apple_music_path(&format!(r"{PACKAGE}\AppleMusic.exe")));
        assert!(is_apple_music_path(
            &format!(r"{PACKAGE}\AppleMusic.exe").to_uppercase()
        ));
        assert!(is_apple_music_path(
            &format!(r"{PACKAGE}\AppleMusic.exe").replace('\\', "/")
        ));
    }

    #[test]
    fn other_apps_and_look_alikes_do_not_match() {
        assert!(!is_apple_music_path(
            r"C:\Program Files\Google\Chrome\Application\chrome.exe"
        ));
        assert!(!is_apple_music_path(
            r"C:\Users\me\AppleMusicWin_fake\player.exe"
        ));
        assert!(!is_apple_music_path(
            r"C:\Program Files\WindowsApps\NotAppleInc.AppleMusicWin_1\x.exe"
        ));
        assert!(!is_apple_music_path(r"C:\Tools\AppleMusic.exe"));
        // The same package folder name anywhere but under WindowsApps is not the app.
        assert!(!is_apple_music_path(
            r"C:\evil\AppleInc.AppleMusicWin_1.0.0.0_x64__x\AppleMusic.exe"
        ));
        assert!(!is_apple_music_path(
            r"C:\Users\me\Downloads\WindowsApps2\AppleInc.AppleMusicWin_1\x.exe"
        ));
        assert!(!is_apple_music_path(""));
    }

    #[test]
    fn the_active_session_is_the_primary() {
        let candidates = [(false, "dormant"), (true, "audible"), (false, "other")];
        assert_eq!(pick_primary(&candidates, None), Some(1));
        assert_eq!(
            pick_primary(&candidates, Some("dormant")),
            Some(1),
            "active beats memory"
        );
    }

    #[test]
    fn when_paused_the_last_active_session_is_the_primary() {
        let candidates = [(false, "dormant"), (false, "audible")];
        assert_eq!(pick_primary(&candidates, Some("audible")), Some(1));
        assert_eq!(
            pick_primary(&candidates, Some("gone")),
            Some(0),
            "unknown memory: first"
        );
        assert_eq!(pick_primary(&candidates, None), Some(0));
    }

    #[test]
    fn no_sessions_means_no_primary() {
        assert_eq!(pick_primary(&[], None), None);
        assert_eq!(pick_primary(&[], Some("audible")), None);
    }
}
