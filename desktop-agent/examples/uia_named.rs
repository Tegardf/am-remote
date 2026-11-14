//! Probe (read-only apart from navigating): opens a link, then lists every visible element of Apple
//! Music's main window whose Name equals the given text, with type, class, AutomationId and position.
//! Finds what identifies "this is the page of <album>".
//!   cargo run --example uia_named -- <music:// link> <name>

use std::process::{Command, Stdio};
use std::time::Duration;

use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, TreeScope_Children, TreeScope_Descendants, UIA_NamePropertyId,
    UIA_ProcessIdPropertyId,
};

use am_remote_agent::media::audio::{list_sessions, APPLE_MUSIC_PROCESS};

fn main() -> windows::core::Result<()> {
    let mut args = std::env::args().skip(1);
    let link = args.next().expect("usage: <link> <name>");
    let name = args.next().expect("usage: <link> <name>");
    // SAFETY: first COM call on the main thread; plain COM / UI Automation calls below.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let pid = list_sessions()?
            .into_iter()
            .find(|s| s.process_name.as_deref() == Some(APPLE_MUSIC_PROCESS))
            .map(|s| s.pid)
            .expect("AppleMusic.exe has no audio session");
        let _ = Command::new("rundll32.exe")
            .arg("url.dll,FileProtocolHandler")
            .arg(&link)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        std::thread::sleep(Duration::from_secs(5));

        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let by_pid = automation
            .CreatePropertyCondition(UIA_ProcessIdPropertyId, &VARIANT::from(pid as i32))?;
        let windows = automation
            .GetRootElement()?
            .FindAll(TreeScope_Children, &by_pid)?;
        let by_name = automation
            .CreatePropertyCondition(UIA_NamePropertyId, &VARIANT::from(name.as_str()))?;
        for w in 0..windows.Length()? {
            let window = windows.GetElement(w)?;
            if window.CurrentIsOffscreen()?.as_bool() {
                continue;
            }
            let found = window.FindAll(TreeScope_Descendants, &by_name)?;
            println!("window {w}: {} elements named {name:?}", found.Length()?);
            for i in 0..found.Length()? {
                let el = found.GetElement(i)?;
                let r = el.CurrentBoundingRectangle()?;
                println!(
                    "  type={} class={:?} id={:?} offscreen={} rect=({},{})-({},{})",
                    el.CurrentControlType()?.0,
                    el.CurrentClassName()?.to_string(),
                    el.CurrentAutomationId()?.to_string(),
                    el.CurrentIsOffscreen()?.as_bool(),
                    r.left,
                    r.top,
                    r.right,
                    r.bottom
                );
            }
        }
    }
    Ok(())
}
