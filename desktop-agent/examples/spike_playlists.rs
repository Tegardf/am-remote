//! Spike probe (navigation only, plays nothing): selects the "Playlists" item of Apple Music's
//! sidebar through UI Automation and lists what the page shows, to see whether the user's library
//! playlists can be read and addressed. `cargo run --example spike_playlists`

use std::time::Duration;

use windows::core::Interface;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationSelectionItemPattern,
    TreeScope_Children, TreeScope_Descendants, UIA_AutomationIdPropertyId,
    UIA_ListItemControlTypeId, UIA_NamePropertyId, UIA_ProcessIdPropertyId,
    UIA_SelectionItemPatternId,
};

use am_remote_agent::media::audio::{list_sessions, APPLE_MUSIC_PROCESS};

fn main() -> windows::core::Result<()> {
    // SAFETY: first COM call on the main thread; plain COM / UI Automation calls below.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let pid = list_sessions()?
            .into_iter()
            .find(|s| s.process_name.as_deref() == Some(APPLE_MUSIC_PROCESS))
            .map(|s| s.pid)
            .expect("AppleMusic.exe has no audio session; play something once first");
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let by_pid = automation
            .CreatePropertyCondition(UIA_ProcessIdPropertyId, &VARIANT::from(pid as i32))?;
        let windows = automation
            .GetRootElement()?
            .FindAll(TreeScope_Children, &by_pid)?;
        let mut main: Option<IUIAutomationElement> = None;
        for w in 0..windows.Length()? {
            let window = windows.GetElement(w)?;
            if !window.CurrentIsOffscreen()?.as_bool() {
                main = Some(window);
                break;
            }
        }
        let main = main.expect("no visible Apple Music window");

        let by_name =
            automation.CreatePropertyCondition(UIA_NamePropertyId, &VARIANT::from("Playlists"))?;
        let nav = main.FindFirst(TreeScope_Descendants, &by_name)?;
        println!(
            "sidebar item: name={:?} class={:?}",
            nav.CurrentName()?.to_string(),
            nav.CurrentClassName()?.to_string()
        );
        nav.GetCurrentPattern(UIA_SelectionItemPatternId)
            .and_then(|p| p.cast::<IUIAutomationSelectionItemPattern>())?
            .Select()?;
        std::thread::sleep(Duration::from_millis(2500));

        let all = automation.CreateTrueCondition()?;
        let elements = main.FindAll(TreeScope_Descendants, &all)?;
        println!("{} elements after navigating", elements.Length()?);
        let mut shown = 0;
        for i in 0..elements.Length()? {
            let el = elements.GetElement(i)?;
            if el.CurrentIsOffscreen()?.as_bool() {
                continue;
            }
            let kind = el.CurrentControlType()?.0;
            let class = el.CurrentClassName()?.to_string();
            let name = el.CurrentName()?.to_string();
            let id = el.CurrentAutomationId()?.to_string();
            // List and grid items, headings and anything with an automation id.
            let interesting = kind == UIA_ListItemControlTypeId.0
                || class.contains("GridViewItem")
                || class.contains("ListViewItem")
                || (!id.is_empty() && kind != 50020)
                || (kind == 50020 && !name.is_empty() && shown < 40);
            if interesting && shown < 90 {
                shown += 1;
                println!(
                    "  type={kind} class={class:?} name={:?} id={id:?}",
                    name.chars().take(80).collect::<String>()
                );
            }
        }
        let _ = UIA_AutomationIdPropertyId;
    }
    Ok(())
}
