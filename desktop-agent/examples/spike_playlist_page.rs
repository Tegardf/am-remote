//! Probe (navigation only, plays nothing): selects the Nth library playlist in Apple Music's sidebar
//! and lists what the content area shows, to find a reliable "this is the right page" signal.
//! `cargo run --example spike_playlist_page -- [index]` (0-based among the library playlists)

use std::time::Duration;

use windows::core::Interface;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationSelectionItemPattern,
    TreeScope_Children, TreeScope_Descendants, UIA_AutomationIdPropertyId, UIA_ClassNamePropertyId,
    UIA_ListItemControlTypeId, UIA_SelectionItemPatternId,
};

use am_remote_agent::media::audio::{is_apple_music_path, process_path};

fn main() -> windows::core::Result<()> {
    let index: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(0);
    // SAFETY: first COM call on the main thread; plain COM / UI Automation calls below.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let by_class = automation.CreatePropertyCondition(
            UIA_ClassNamePropertyId,
            &VARIANT::from("WinUIDesktopWin32WindowClass"),
        )?;
        let windows = automation
            .GetRootElement()?
            .FindAll(TreeScope_Children, &by_class)?;
        let mut main: Option<IUIAutomationElement> = None;
        for w in 0..windows.Length()? {
            let window = windows.GetElement(w)?;
            let belongs = process_path(window.CurrentProcessId()? as u32)
                .is_some_and(|p| is_apple_music_path(&p));
            if belongs && !window.CurrentIsOffscreen()?.as_bool() {
                main = Some(window);
                break;
            }
        }
        let main = main.expect("no visible Apple Music window");

        let by_id = |id: &str| -> windows::core::Result<IUIAutomationElement> {
            let cond = automation
                .CreatePropertyCondition(UIA_AutomationIdPropertyId, &VARIANT::from(id))?;
            main.FindFirst(TreeScope_Descendants, &cond)
        };
        let select = |el: &IUIAutomationElement| -> windows::core::Result<()> {
            el.GetCurrentPattern(UIA_SelectionItemPatternId)
                .and_then(|p| p.cast::<IUIAutomationSelectionItemPattern>())?
                .Select()
        };

        select(&by_id("Sidebar_Header_Playlists")?)?;
        std::thread::sleep(Duration::from_millis(2000));

        // The library playlists in the sidebar, in order.
        let items_cond = automation.CreatePropertyCondition(
            windows::Win32::UI::Accessibility::UIA_ControlTypePropertyId,
            &VARIANT::from(UIA_ListItemControlTypeId.0),
        )?;
        let items = main.FindAll(TreeScope_Descendants, &items_cond)?;
        let mut playlists = Vec::new();
        for i in 0..items.Length()? {
            let el = items.GetElement(i)?;
            if el
                .CurrentAutomationId()?
                .to_string()
                .ends_with("IKIND:ePlaylist")
            {
                playlists.push(el);
            }
        }
        println!("{} playlists realised in the sidebar", playlists.len());
        let target = &playlists[index.min(playlists.len() - 1)];
        let target_name = target.CurrentName()?.to_string();
        println!(
            "selecting playlist #{index}: name={:?}",
            target.CurrentName()?.to_string()
        );
        let sidebar_right = by_id("MenuItemsScrollViewer")?
            .CurrentBoundingRectangle()?
            .right;
        let transport_top = by_id("TransportControl_PlayPauseStop")?
            .CurrentBoundingRectangle()?
            .bottom;
        select(target)?;
        std::thread::sleep(Duration::from_millis(2500));

        println!("content area: left>={sidebar_right}, top>={transport_top}");
        let all = automation.CreateTrueCondition()?;
        let elements = main.FindAll(TreeScope_Descendants, &all)?;
        let mut shown = 0;
        for i in 0..elements.Length()? {
            let el = elements.GetElement(i)?;
            if el.CurrentIsOffscreen()?.as_bool() {
                continue;
            }
            let rect = el.CurrentBoundingRectangle()?;
            if rect.left < sidebar_right || rect.top < transport_top {
                continue;
            }
            let name = el.CurrentName()?.to_string();
            let id = el.CurrentAutomationId()?.to_string();
            if (id.is_empty() && name != target_name) || id == "ItemSwipeContainer" || shown >= 40 {
                continue;
            }
            shown += 1;
            println!(
                "  type={} class={:?} name={:?} id={id:?}",
                el.CurrentControlType()?.0,
                el.CurrentClassName()?.to_string(),
                name.chars().take(70).collect::<String>()
            );
        }
    }
    Ok(())
}
