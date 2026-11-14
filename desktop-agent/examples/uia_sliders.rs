//! Probe (read-only): lists every Slider in Apple Music's window(s) through UI Automation, with
//! its name, automation id and range value. Used to find Apple Music's in-app volume slider.
//! Run with Apple Music open: `cargo run --example uia_sliders`.

use windows::core::Interface;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationRangeValuePattern, TreeScope_Children,
    TreeScope_Descendants, UIA_ControlTypePropertyId, UIA_ProcessIdPropertyId,
    UIA_RangeValuePatternId, UIA_SliderControlTypeId,
};

use am_remote_agent::media::audio;

fn main() -> windows::core::Result<()> {
    // SAFETY: first COM call on the main thread.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
    let apple = audio::list_sessions()?
        .into_iter()
        .find(|s| s.process_name.as_deref() == Some(audio::APPLE_MUSIC_PROCESS));
    let Some(apple) = apple else {
        println!("AppleMusic.exe has no audio session; play something first");
        return Ok(());
    };
    println!("AppleMusic.exe pid {}", apple.pid);
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
        let root = automation.GetRootElement()?;
        let by_pid = automation
            .CreatePropertyCondition(UIA_ProcessIdPropertyId, &VARIANT::from(apple.pid as i32))?;
        let windows = root.FindAll(TreeScope_Children, &by_pid)?;
        println!("{} top-level window(s) of Apple Music", windows.Length()?);
        let sliders = automation.CreatePropertyCondition(
            UIA_ControlTypePropertyId,
            &VARIANT::from(UIA_SliderControlTypeId.0),
        )?;
        for w in 0..windows.Length()? {
            let window = windows.GetElement(w)?;
            println!("window {w}: name={:?}", window.CurrentName()?.to_string());
            let found = window.FindAll(TreeScope_Descendants, &sliders)?;
            for i in 0..found.Length()? {
                let el = found.GetElement(i)?;
                let range = el
                    .GetCurrentPattern(UIA_RangeValuePatternId)
                    .and_then(|p| p.cast::<IUIAutomationRangeValuePattern>());
                let value = match range {
                    Ok(r) => format!(
                        "value={} min={} max={}",
                        r.CurrentValue()?,
                        r.CurrentMinimum()?,
                        r.CurrentMaximum()?
                    ),
                    Err(_) => "(no RangeValue pattern)".into(),
                };
                println!(
                    "  slider name={:?} automation_id={:?} {value}",
                    el.CurrentName()?.to_string(),
                    el.CurrentAutomationId()?.to_string()
                );
            }
        }
    }
    Ok(())
}
