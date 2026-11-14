//! Spike probe: open a link, then act on the page through UI Automation, and watch SMTC.
//! STARTS MUSIC on this laptop for `play` and `row`; it pauses at the end.
//!
//!   cargo run --example spike_link_click -- <url> play        click the page Play button
//!   cargo run --example spike_link_click -- <url> rows        list the track rows and their patterns
//!   cargo run --example spike_link_click -- <url> row <n>     invoke row n (0-based) through UIA
//!
//! Public OS APIs only (ShellExecute via rundll32, UI Automation, SMTC). Everything has a timeout.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use windows::core::Interface;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern,
    IUIAutomationLegacyIAccessiblePattern, IUIAutomationScrollPattern,
    IUIAutomationSelectionItemPattern, ScrollAmount_LargeIncrement, ScrollAmount_NoAmount,
    TreeScope_Children, TreeScope_Descendants, UIA_AutomationIdPropertyId,
    UIA_ControlTypePropertyId, UIA_InvokePatternId, UIA_IsScrollPatternAvailablePropertyId,
    UIA_LegacyIAccessiblePatternId, UIA_ListItemControlTypeId, UIA_MenuItemControlTypeId,
    UIA_NamePropertyId, UIA_ProcessIdPropertyId, UIA_ScrollPatternId, UIA_SelectionItemPatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VK_RETURN,
};

use am_remote_agent::media::audio::{list_sessions, APPLE_MUSIC_PROCESS};
use am_remote_agent::media::{MediaBackend, PlaybackStatus, Transport, WindowsBackend};

const PAGE_WAIT: Duration = Duration::from_secs(5);
const PLAY_WAIT: Duration = Duration::from_secs(10);

/// One Enter key press (down, up) through SendInput.
fn press_enter() {
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_RETURN,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)];
    // SAFETY: plain SendInput call with a valid, correctly sized array.
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let url = args.next().expect("usage: <url> play|rows|row <n>");
    let action = args.next().expect("usage: <url> play|rows|row <n>");
    let row_arg = args.next().unwrap_or_default();
    let row: usize = row_arg.parse().unwrap_or(0);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let backend = WindowsBackend::start()?;
    // SAFETY: first COM call on the main thread (a separate apartment from the media thread).
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };

    for _ in 0..10 {
        runtime.block_on(backend.transport(Transport::Pause)).ok();
        std::thread::sleep(Duration::from_millis(600));
        if runtime.block_on(backend.state()).status != PlaybackStatus::Playing {
            break;
        }
    }
    let before = runtime.block_on(backend.state());
    if before.status == PlaybackStatus::Playing {
        println!("could not pause first; aborting");
        return Ok(());
    }
    println!("before: {:?} ({:?})", before.title, before.status);

    let _ = Command::new("rundll32.exe")
        .arg("url.dll,FileProtocolHandler")
        .arg(&url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    std::thread::sleep(PAGE_WAIT);

    let pid = list_sessions()?
        .into_iter()
        .find(|s| s.process_name.as_deref() == Some(APPLE_MUSIC_PROCESS))
        .map(|s| s.pid)
        .expect("AppleMusic.exe has no audio session");

    // SAFETY: plain COM / UI Automation calls on interfaces obtained below.
    unsafe {
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let by_pid = automation
            .CreatePropertyCondition(UIA_ProcessIdPropertyId, &VARIANT::from(pid as i32))?;
        let windows = automation
            .GetRootElement()?
            .FindAll(TreeScope_Children, &by_pid)?;
        // The main window is the first visible one.
        let mut main: Option<IUIAutomationElement> = None;
        for w in 0..windows.Length()? {
            let window = windows.GetElement(w)?;
            if !window.CurrentIsOffscreen()?.as_bool() {
                main = Some(window);
                break;
            }
        }
        let main = main.expect("no visible Apple Music window");

        let target: Option<IUIAutomationElement> = match action.as_str() {
            "play" => {
                let cond = automation.CreatePropertyCondition(
                    UIA_AutomationIdPropertyId,
                    &VARIANT::from("PlayButtonElement"),
                )?;
                main.FindFirst(TreeScope_Descendants, &cond).ok()
            }
            "rowscroll" => {
                // The page is the largest vertically scrollable element.
                let cond = automation.CreatePropertyCondition(
                    UIA_IsScrollPatternAvailablePropertyId,
                    &VARIANT::from(true),
                )?;
                let found = main.FindAll(TreeScope_Descendants, &cond)?;
                let mut page: Option<(i64, IUIAutomationScrollPattern)> = None;
                for i in 0..found.Length()? {
                    let el = found.GetElement(i)?;
                    let scroll = el
                        .GetCurrentPattern(UIA_ScrollPatternId)
                        .and_then(|p| p.cast::<IUIAutomationScrollPattern>())?;
                    if !scroll.CurrentVerticallyScrollable()?.as_bool() {
                        continue;
                    }
                    let r = el.CurrentBoundingRectangle()?;
                    let area = i64::from(r.right - r.left) * i64::from(r.bottom - r.top);
                    if page.as_ref().is_none_or(|(a, _)| area > *a) {
                        page = Some((area, scroll));
                    }
                }
                let (_, page) = page.expect("no scrollable page");
                let rows_cond = automation.CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &VARIANT::from(UIA_ListItemControlTypeId.0),
                )?;
                let mut target: Option<IUIAutomationElement> = None;
                for attempt in 0..20 {
                    let rows = main.FindAll(TreeScope_Descendants, &rows_cond)?;
                    let mut best: Option<(i32, IUIAutomationElement)> = None;
                    for i in 0..rows.Length()? {
                        let el = rows.GetElement(i)?;
                        if el.CurrentClassName()? == "ListViewItem"
                            && el.CurrentName()?.to_string().starts_with(&row_arg)
                        {
                            let left = el.CurrentBoundingRectangle()?.left;
                            if best.as_ref().is_none_or(|(l, _)| left < *l) {
                                best = Some((left, el));
                            }
                        }
                    }
                    if let Some((_, el)) = best {
                        println!(
                            "found the row after {attempt} scrolls (page at {:.0}%)",
                            page.CurrentVerticalScrollPercent()?
                        );
                        target = Some(el);
                        break;
                    }
                    println!(
                        "attempt {attempt}: ListItems={} page at {:.0}%",
                        rows.Length()?,
                        page.CurrentVerticalScrollPercent()?
                    );
                    page.Scroll(ScrollAmount_NoAmount, ScrollAmount_LargeIncrement)?;
                    std::thread::sleep(Duration::from_millis(350));
                }
                target
            }
            "scrollers" => {
                let cond = automation.CreatePropertyCondition(
                    UIA_IsScrollPatternAvailablePropertyId,
                    &VARIANT::from(true),
                )?;
                let found = main.FindAll(TreeScope_Descendants, &cond)?;
                println!("{} scrollable elements", found.Length()?);
                for i in 0..found.Length()? {
                    let el = found.GetElement(i)?;
                    let r = el.CurrentBoundingRectangle()?;
                    let scroll = el
                        .GetCurrentPattern(UIA_ScrollPatternId)
                        .and_then(|p| p.cast::<IUIAutomationScrollPattern>())?;
                    println!(
                        "  id={:?} class={:?} rect=({},{})-({},{}) vertical={} v%={:.0} offscreen={}",
                        el.CurrentAutomationId()?.to_string(),
                        el.CurrentClassName()?.to_string(),
                        r.left, r.top, r.right, r.bottom,
                        scroll.CurrentVerticallyScrollable()?.as_bool(),
                        scroll.CurrentVerticalScrollPercent()?,
                        el.CurrentIsOffscreen()?.as_bool()
                    );
                }
                return Ok(());
            }
            "rows" | "row" | "rowenter" | "rowdefault" | "rowkids" | "rowmenu" => {
                let cond = automation.CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &VARIANT::from(UIA_ListItemControlTypeId.0),
                )?;
                let rows = main.FindAll(TreeScope_Descendants, &cond)?;
                println!("ListItems in the tree (any visibility): {}", rows.Length()?);
                if std::env::var("DUMP_ALL").is_ok() {
                    for i in 0..rows.Length()? {
                        let el = rows.GetElement(i)?;
                        let r = el.CurrentBoundingRectangle()?;
                        println!(
                            "  all[{i}] class={:?} off={} x={} y={} name={:?}",
                            el.CurrentClassName()?
                                .to_string()
                                .rsplit('.')
                                .next()
                                .unwrap_or(""),
                            el.CurrentIsOffscreen()?.as_bool(),
                            r.left,
                            r.top,
                            el.CurrentName()?
                                .to_string()
                                .chars()
                                .take(48)
                                .collect::<String>()
                        );
                    }
                }
                for i in 0..rows.Length()? {
                    let el = rows.GetElement(i)?;
                    let name = el.CurrentName()?.to_string();
                    if el.CurrentClassName()? == "ListViewItem" && name.starts_with("Jatuh Suka") {
                        let r = el.CurrentBoundingRectangle()?;
                        println!(
                            "  found {:?} offscreen={} rect=({},{})-({},{})",
                            name.chars().take(50).collect::<String>(),
                            el.CurrentIsOffscreen()?.as_bool(),
                            r.left,
                            r.top,
                            r.right,
                            r.bottom
                        );
                    }
                }
                let mut visible = Vec::new();
                for i in 0..rows.Length()? {
                    let el = rows.GetElement(i)?;
                    if !el.CurrentIsOffscreen()?.as_bool() {
                        visible.push(el);
                    }
                }
                println!("{} visible list rows", visible.len());
                for (n, el) in visible.iter().enumerate().take(15) {
                    let invoke = el.GetCurrentPattern(UIA_InvokePatternId).is_ok();
                    let select = el.GetCurrentPattern(UIA_SelectionItemPatternId).is_ok();
                    println!(
                        "  row {n}: name={:?} class={:?} invoke={invoke} select={select}",
                        el.CurrentName()?
                            .to_string()
                            .chars()
                            .take(90)
                            .collect::<String>(),
                        el.CurrentClassName()?.to_string()
                    );
                }
                if action == "rowkids" || action == "rowmenu" {
                    let el = visible.into_iter().nth(row).expect("no such row");
                    println!("row {row}: {:?}", el.CurrentName()?.to_string());
                    let all = automation.CreateTrueCondition()?;
                    let kids = el.FindAll(TreeScope_Descendants, &all)?;
                    let mut more: Option<IUIAutomationElement> = None;
                    for i in 0..kids.Length()? {
                        let kid = kids.GetElement(i)?;
                        let name = kid.CurrentName()?.to_string();
                        println!(
                            "  child type={} name={:?} id={:?} offscreen={} invoke={}",
                            kid.CurrentControlType()?.0,
                            name.chars().take(60).collect::<String>(),
                            kid.CurrentAutomationId()?.to_string(),
                            kid.CurrentIsOffscreen()?.as_bool(),
                            kid.GetCurrentPattern(UIA_InvokePatternId).is_ok()
                        );
                        if name == "More" && more.is_none() {
                            more = Some(kid);
                        }
                    }
                    if action == "rowmenu" {
                        let preset = more;
                        let row_rect = el.CurrentBoundingRectangle()?;
                        let by_name = automation
                            .CreatePropertyCondition(UIA_NamePropertyId, &VARIANT::from("More"))?;
                        let candidates = main.FindAll(TreeScope_Descendants, &by_name)?;
                        let mut found: Option<IUIAutomationElement> = preset;
                        for i in 0..candidates.Length()? {
                            let button = candidates.GetElement(i)?;
                            let rect = button.CurrentBoundingRectangle()?;
                            let middle = (rect.top + rect.bottom) / 2;
                            println!(
                                "  More candidate: y={middle} (row spans {}..{})",
                                row_rect.top, row_rect.bottom
                            );
                            if middle >= row_rect.top
                                && middle <= row_rect.bottom
                                && found.is_none()
                            {
                                found = Some(button);
                            }
                        }
                        let more = found.expect("no More button lines up with this row");
                        let invoke = more
                            .GetCurrentPattern(UIA_InvokePatternId)
                            .and_then(|p| p.cast::<IUIAutomationInvokePattern>())?;
                        println!("opening the More menu");
                        invoke.Invoke()?;
                        std::thread::sleep(Duration::from_millis(1200));
                        let menu_items = automation.CreatePropertyCondition(
                            UIA_ControlTypePropertyId,
                            &VARIANT::from(UIA_MenuItemControlTypeId.0),
                        )?;
                        // Popups may be separate top-level windows of the app.
                        let windows = automation
                            .GetRootElement()?
                            .FindAll(TreeScope_Children, &by_pid)?;
                        for w in 0..windows.Length()? {
                            let window = windows.GetElement(w)?;
                            let items = window.FindAll(TreeScope_Descendants, &menu_items)?;
                            for i in 0..items.Length()? {
                                let item = items.GetElement(i)?;
                                println!(
                                    "  menu item (window {w}): name={:?} id={:?} offscreen={} invoke={}",
                                    item.CurrentName()?.to_string(),
                                    item.CurrentAutomationId()?.to_string(),
                                    item.CurrentIsOffscreen()?.as_bool(),
                                    item.GetCurrentPattern(UIA_InvokePatternId).is_ok()
                                );
                            }
                        }
                        // Close the menu again by toggling the same button.
                        let _ = invoke.Invoke();
                    }
                    return Ok(());
                }
                if action == "row" || action == "rowenter" || action == "rowdefault" {
                    if row_arg.parse::<usize>().is_ok() {
                        visible.into_iter().nth(row)
                    } else {
                        // A name prefix: the leftmost ListViewItem that starts with it (the play
                        // queue pane is on the right and can list the same song).
                        let mut best: Option<(i32, IUIAutomationElement)> = None;
                        for el in visible {
                            let name = el.CurrentName()?.to_string();
                            if name.starts_with(&row_arg)
                                && el.CurrentClassName()? == "ListViewItem"
                            {
                                let left = el.CurrentBoundingRectangle()?.left;
                                if best.as_ref().is_none_or(|(l, _)| left < *l) {
                                    best = Some((left, el));
                                }
                            }
                        }
                        best.map(|(_, el)| el)
                    }
                } else {
                    None
                }
            }
            other => panic!("unknown action {other}"),
        };

        if action == "rows" {
            return Ok(());
        }
        let Some(target) = target else {
            println!("target element not found");
            return Ok(());
        };
        println!("target: name={:?}", target.CurrentName()?.to_string());
        if action == "rowdefault" {
            // Straight to the legacy default action ("Double Click"), no keyboard, no Invoke.
            let legacy = target
                .GetCurrentPattern(UIA_LegacyIAccessiblePatternId)
                .and_then(|p| p.cast::<IUIAutomationLegacyIAccessiblePattern>());
            match legacy {
                Ok(legacy) => {
                    println!(
                        "legacy default action = {:?}",
                        legacy.CurrentDefaultAction()?.to_string()
                    );
                    legacy.DoDefaultAction()?;
                    println!("did the default action");
                }
                Err(e) => println!("no LegacyIAccessible pattern: {e}"),
            }
        } else if action == "rowenter" {
            // Select, focus, then one Enter key press: the keyboard route, never Invoke.
            let select = target
                .GetCurrentPattern(UIA_SelectionItemPatternId)
                .and_then(|p| p.cast::<IUIAutomationSelectionItemPattern>())?;
            select.Select()?;
            target.SetFocus()?;
            std::thread::sleep(Duration::from_millis(400));
            println!("selected and focused; pressing Enter");
            press_enter();
        } else if let Ok(pattern) = target
            .GetCurrentPattern(UIA_InvokePatternId)
            .and_then(|p| p.cast::<IUIAutomationInvokePattern>())
        {
            println!("invoking via InvokePattern");
            pattern.Invoke()?;
        } else if let Ok(pattern) = target
            .GetCurrentPattern(UIA_SelectionItemPatternId)
            .and_then(|p| p.cast::<IUIAutomationSelectionItemPattern>())
        {
            println!("no Invoke: selecting via SelectionItemPattern");
            pattern.Select()?;
            if action == "rowdefault" || action == "rowscroll" {
                // No keyboard: ask the row for its legacy MSAA default action.
                let legacy = target
                    .GetCurrentPattern(UIA_LegacyIAccessiblePatternId)
                    .and_then(|p| p.cast::<IUIAutomationLegacyIAccessiblePattern>());
                match legacy {
                    Ok(legacy) => {
                        println!(
                            "legacy default action = {:?}",
                            legacy.CurrentDefaultAction()?.to_string()
                        );
                        legacy.DoDefaultAction()?;
                        println!("did the default action");
                    }
                    Err(e) => println!("no LegacyIAccessible pattern: {e}"),
                }
            }
            if action == "rowenter" {
                // Give the row keyboard focus, then press Enter once, like a user would.
                target.SetFocus()?;
                std::thread::sleep(Duration::from_millis(400));
                println!("pressing Enter");
                press_enter();
            }
        } else {
            println!("target supports neither Invoke nor SelectionItem");
            return Ok(());
        }
    }

    let started = Instant::now();
    let mut result = None;
    while started.elapsed() < PLAY_WAIT {
        std::thread::sleep(Duration::from_millis(400));
        let now = runtime.block_on(backend.state());
        if now.status == PlaybackStatus::Playing && !now.title.starts_with("Connecting") {
            result = Some((
                now.title,
                now.album,
                now.artist,
                started.elapsed().as_millis(),
            ));
            break;
        }
    }
    match result {
        Some((title, album, artist, ms)) => {
            println!("PLAYING after {ms} ms: title={title:?} artist={artist:?} album={album:?}")
        }
        None => println!("no playback within 10 s"),
    }
    runtime.block_on(backend.transport(Transport::Pause)).ok();
    println!("paused");
    Ok(())
}
