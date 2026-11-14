//! Spike probe (read-only apart from navigating): opens one link with `rundll32 url.dll,
//! FileProtocolHandler`, waits, then prints what Apple Music's main window shows through UI
//! Automation: the visible buttons (with their automation ids and whether they can be invoked) and the
//! first visible texts. Tells whether the link navigated to the item and whether a Play button is
//! reachable. It never clicks anything.
//!   cargo run --example spike_link_page -- <url> [seconds-to-wait]

use std::process::{Command, Stdio};
use std::time::Duration;

use windows::core::Interface;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationInvokePattern, TreeScope_Children,
    TreeScope_Descendants, UIA_ButtonControlTypeId, UIA_HyperlinkControlTypeId,
    UIA_InvokePatternId, UIA_ProcessIdPropertyId, UIA_TextControlTypeId,
};

use am_remote_agent::media::audio::{list_sessions, APPLE_MUSIC_PROCESS};

fn main() -> windows::core::Result<()> {
    let mut args = std::env::args().skip(1);
    let url = args.next().expect("usage: spike_link_page <url> [seconds]");
    let wait: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(5);

    // SAFETY: first COM call on the main thread; plain COM / UI Automation calls below.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let pid = list_sessions()?
            .into_iter()
            .find(|s| s.process_name.as_deref() == Some(APPLE_MUSIC_PROCESS))
            .map(|s| s.pid)
            .expect("AppleMusic.exe has no audio session; play something once first");

        println!("opening {url}");
        let _ = Command::new("rundll32.exe")
            .arg("url.dll,FileProtocolHandler")
            .arg(&url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        std::thread::sleep(Duration::from_secs(wait));

        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let by_pid = automation
            .CreatePropertyCondition(UIA_ProcessIdPropertyId, &VARIANT::from(pid as i32))?;
        let windows = automation
            .GetRootElement()?
            .FindAll(TreeScope_Children, &by_pid)?;
        for w in 0..windows.Length()? {
            let window = windows.GetElement(w)?;
            if window.CurrentIsOffscreen()?.as_bool() {
                continue;
            }
            println!("window {w}: {:?}", window.CurrentName()?.to_string());
            let all = automation.CreateTrueCondition()?;
            let elements = window.FindAll(TreeScope_Descendants, &all)?;
            let (mut texts, mut buttons) = (0, 0);
            for i in 0..elements.Length()? {
                let el = elements.GetElement(i)?;
                if el.CurrentIsOffscreen()?.as_bool() {
                    continue;
                }
                let kind = el.CurrentControlType()?;
                let name = el.CurrentName()?.to_string();
                let id = el.CurrentAutomationId()?.to_string();
                if kind == UIA_ButtonControlTypeId || kind == UIA_HyperlinkControlTypeId {
                    let invokable = el
                        .GetCurrentPattern(UIA_InvokePatternId)
                        .and_then(|p| p.cast::<IUIAutomationInvokePattern>())
                        .is_ok();
                    if buttons < 60 && (!name.is_empty() || !id.is_empty()) {
                        buttons += 1;
                        println!("  button name={name:?} id={id:?} invokable={invokable}");
                    }
                } else if kind == UIA_TextControlTypeId && !name.is_empty() && texts < 25 {
                    texts += 1;
                    println!("  text   {name:?}");
                }
            }
        }
    }
    Ok(())
}
