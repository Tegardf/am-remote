//! Probe (read-only): finds every UI Automation element of Apple Music whose name or automation id
//! mentions "volume"/"mute"/"sound", or that has a RangeValue pattern, of any control type.
//! Run with Apple Music open (try it in normal, mini-player and full-screen mode):
//! `cargo run --example uia_volume`.

use windows::core::Interface;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationRangeValuePattern, TreeScope_Children,
    TreeScope_Descendants, UIA_ProcessIdPropertyId, UIA_RangeValuePatternId,
};

use am_remote_agent::media::audio;

fn main() -> windows::core::Result<()> {
    // SAFETY: first COM call on the main thread.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let Some(apple) = audio::list_sessions()?
        .into_iter()
        .find(|s| s.process_name.as_deref() == Some(audio::APPLE_MUSIC_PROCESS))
    else {
        println!("AppleMusic.exe has no audio session; play something first");
        return Ok(());
    };
    if let Some(v) = audio::AppleMusicVolume::default().get()? {
        println!(
            "Core Audio per-app volume (Windows mixer): {}% muted={}",
            v.level, v.muted
        );
    }
    // SAFETY: plain COM / UI Automation calls.
    unsafe {
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let by_pid = automation
            .CreatePropertyCondition(UIA_ProcessIdPropertyId, &VARIANT::from(apple.pid as i32))?;
        let windows = automation
            .GetRootElement()?
            .FindAll(TreeScope_Children, &by_pid)?;
        let all = automation.CreateTrueCondition()?;
        for w in 0..windows.Length()? {
            let window = windows.GetElement(w)?;
            let (rect, class) = (
                window.CurrentBoundingRectangle()?,
                window.CurrentClassName()?.to_string(),
            );
            println!(
                "window {w}: class={class:?} rect=({},{})-({},{}) offscreen={}",
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
                window.CurrentIsOffscreen()?.as_bool()
            );
            let elements = window.FindAll(TreeScope_Descendants, &all)?;
            println!("  {} elements", elements.Length()?);
            for i in 0..elements.Length()? {
                let el = elements.GetElement(i)?;
                let name = el.CurrentName()?.to_string();
                let id = el.CurrentAutomationId()?.to_string();
                let text = format!("{name} {id}").to_lowercase();
                let range = el
                    .GetCurrentPattern(UIA_RangeValuePatternId)
                    .and_then(|p| p.cast::<IUIAutomationRangeValuePattern>())
                    .ok();
                let interesting = ["volume", "mute", "sound", "speaker"]
                    .iter()
                    .any(|k| text.contains(k));
                if interesting || range.is_some() {
                    let value = range
                        .map(|r| {
                            format!(
                                "value={} min={} max={}",
                                r.CurrentValue().unwrap_or(-1.0),
                                r.CurrentMinimum().unwrap_or(-1.0),
                                r.CurrentMaximum().unwrap_or(-1.0)
                            )
                        })
                        .unwrap_or_default();
                    println!(
                        "  type={} name={name:?} id={id:?} class={:?} offscreen={} {value}",
                        el.CurrentControlType()?.0,
                        el.CurrentClassName()?.to_string(),
                        el.CurrentIsOffscreen()?.as_bool()
                    );
                }
            }
        }
    }
    Ok(())
}
