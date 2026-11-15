//! MVP 4 spike, C: searching through Apple Music's own search UI with UI Automation only
//! (patterns, no typing, no mouse). Every action prints the elements that APPEARED because of it.
//!   cargo run --example spike_mvp4_search -- open              invoke the sidebar search button
//!   cargo run --example spike_mvp4_search -- set <text>        ValuePattern.SetValue on the search box
//!   cargo run --example spike_mvp4_search -- results [filter]  list the result page elements
//!   cargo run --example spike_mvp4_search -- invoke <id>       invoke the element with this AutomationId
//!   cargo run --example spike_mvp4_search -- back              press the app's Back button

#[path = "spike_common/mod.rs"]
mod common;

use std::collections::HashSet;
use std::time::Instant;

use windows::core::Interface;
use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, IUIAutomationValuePattern, UIA_ValuePatternId,
};

fn key(el: &IUIAutomationElement) -> String {
    // SAFETY: plain UI Automation call.
    let t = unsafe { el.CurrentControlType() }.map(|t| t.0).unwrap_or(0);
    format!(
        "{t}|{}|{}|{}",
        common::id(el),
        common::name(el),
        common::class(el)
    )
}

fn snapshot(ui: &common::Ui) -> Vec<(String, IUIAutomationElement)> {
    common::array_items(&ui.all())
        .into_iter()
        .map(|e| (key(&e), e))
        .collect()
}

fn print_new(before: &HashSet<String>, after: &[(String, IUIAutomationElement)], limit: usize) {
    let new: Vec<_> = after.iter().filter(|(k, _)| !before.contains(k)).collect();
    println!("{} new elements (of {})", new.len(), after.len());
    for (_, el) in new.iter().take(limit) {
        println!("  {}", common::describe(el));
    }
    if new.len() > limit {
        println!("  … {} more", new.len() - limit);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let action = args.first().map(String::as_str).unwrap_or("open");
    let ui = common::start();
    let before_all = snapshot(&ui);
    let before: HashSet<String> = before_all.iter().map(|(k, _)| k.clone()).collect();
    let started = Instant::now();
    match action {
        "open" => {
            let button = before_all
                .iter()
                .find(|(_, e)| common::id(e) == "Search_Button")
                .expect("no Search_Button")
                .1
                .clone();
            println!("Search_Button invoke -> {}", common::invoke(&button));
            common::pause(1500);
            print_new(&before, &snapshot(&ui), 80);
        }
        "set" => {
            let text = args.get(1).expect("usage: set <text>");
            // The search box: an Edit control (50004) that supports ValuePattern.
            let edit = before_all
                .iter()
                .map(|(_, e)| e)
                .find(|e| {
                    // SAFETY: plain UI Automation call.
                    unsafe { e.CurrentControlType() }.map(|t| t.0).unwrap_or(0) == 50004
                        && !common::offscreen(e)
                })
                .expect("no visible Edit control: run `open` first")
                .clone();
            println!("search box: {}", common::describe(&edit));
            // SAFETY: plain UI Automation calls.
            unsafe {
                let result = edit
                    .GetCurrentPattern(UIA_ValuePatternId)
                    .and_then(|p| p.cast::<IUIAutomationValuePattern>())
                    .and_then(|p| p.SetValue(&windows::core::BSTR::from(text.as_str())));
                println!("SetValue({text:?}) -> {result:?}");
            }
            // Poll for the first change in the UI.
            let mut first_change = None;
            for _ in 0..50 {
                common::pause(100);
                if snapshot(&ui).iter().any(|(k, _)| !before.contains(k)) {
                    first_change = Some(started.elapsed().as_millis());
                    break;
                }
            }
            println!("first UI change after {first_change:?} ms");
            common::pause(2500);
            print_new(&before, &snapshot(&ui), 120);
            let edit_now = snapshot(&ui)
                .into_iter()
                .map(|(_, e)| e)
                .find(|e| unsafe { e.CurrentControlType() }.map(|t| t.0).unwrap_or(0) == 50004);
            if let Some(e) = edit_now {
                println!("search box now: {}", common::describe(&e));
            }
        }
        "results" => {
            let filter = args.get(1).map(|s| s.to_lowercase());
            for (_, el) in &before_all {
                if common::offscreen(el) {
                    continue;
                }
                let line = common::describe(el);
                // Only the main content area (right of the sidebar, left of the queue pane).
                if filter
                    .as_ref()
                    .is_none_or(|f| line.to_lowercase().contains(f))
                {
                    println!("{line}");
                }
            }
        }
        "invoke" => {
            // `invoke <id>` or `invoke name:<exact name>[:<class>]` (default class ListViewItem).
            let id = args.get(1).expect("usage: invoke <id> | name:<text>");
            let el = before_all
                .iter()
                .find(|(_, e)| {
                    let item = matches!(common::class(e).as_str(), "ListViewItem" | "GridViewItem");
                    if let Some(name) = id.strip_prefix("name:") {
                        item && common::name(e) == name
                    } else if let Some(part) = id.strip_prefix("has:") {
                        item && common::name(e).contains(part)
                    } else {
                        common::id(e) == *id
                    }
                })
                .unwrap_or_else(|| panic!("no element {id}"))
                .1
                .clone();
            let started = Instant::now();
            println!("invoke {id} -> {}", common::invoke(&el));
            let mut first_change = None;
            for _ in 0..50 {
                common::pause(100);
                if snapshot(&ui).iter().any(|(k, _)| !before.contains(k)) {
                    first_change = Some(started.elapsed().as_millis());
                    break;
                }
            }
            println!("first UI change after {first_change:?} ms");
            common::pause(2500);
            print_new(&before, &snapshot(&ui), 60);
        }
        "sections" => {
            // Walks the result page section by section (each section is a wide ListViewItem),
            // scrolling it into view, and prints its heading and the first items inside it.
            use windows::Win32::UI::Accessibility::{
                IUIAutomationScrollItemPattern, UIA_ScrollItemPatternId,
            };
            let width = |e: &IUIAutomationElement| {
                // SAFETY: plain UI Automation call.
                unsafe { e.CurrentBoundingRectangle() }
                    .map(|r| r.right - r.left)
                    .unwrap_or(0)
            };
            let mut seen: Vec<String> = Vec::new();
            for round in 0..14 {
                let all = snapshot(&ui);
                let sections: Vec<&IUIAutomationElement> = all
                    .iter()
                    .map(|(_, e)| e)
                    .filter(|e| common::class(e) == "ListViewItem" && width(e) > 1000)
                    .collect();
                let mut progressed = false;
                for section in &sections {
                    let title = common::name(section);
                    if seen.contains(&title) {
                        continue;
                    }
                    seen.push(title.clone());
                    progressed = true;
                    // SAFETY: plain UI Automation calls.
                    unsafe {
                        let _ = section
                            .GetCurrentPattern(UIA_ScrollItemPatternId)
                            .and_then(|p| p.cast::<IUIAutomationScrollItemPattern>())
                            .and_then(|p| p.ScrollIntoView());
                    }
                    common::pause(900);
                    let items: Vec<IUIAutomationElement> = common::array_items(&ui.below(section))
                        .into_iter()
                        .filter(|e| {
                            let c = common::class(e);
                            c == "GridViewItem" || c == "ListViewItem"
                        })
                        .collect();
                    println!(
                        "SECTION {title:?}: {} items exposed (round {round})",
                        items.len()
                    );
                    for item in items.iter().take(4) {
                        println!(
                            "    {:?} [{}]",
                            common::truncate(&common::name(item), 100),
                            common::patterns(item)
                        );
                    }
                }
                if !progressed {
                    break;
                }
            }
        }
        "scrollpage" => {
            // Scrolls the whole result page in steps and lists the section headings that exist
            // after each step, with the number of items each exposes.
            use windows::Win32::UI::Accessibility::{
                IUIAutomationScrollPattern, UIA_ScrollPatternId, UIA_ScrollPatternNoScroll,
            };
            let page = before_all
                .iter()
                .map(|(_, e)| e)
                .find(|e| {
                    // SAFETY: plain UI Automation call.
                    let r = unsafe { e.CurrentBoundingRectangle() }.unwrap_or_default();
                    common::class(e) == "ListView"
                        && r.right - r.left > 1000
                        && r.bottom - r.top > 700
                })
                .expect("no page list")
                .clone();
            for percent in [0.0, 20.0, 40.0, 60.0, 80.0, 100.0] {
                // SAFETY: plain UI Automation calls.
                let result = unsafe {
                    page.GetCurrentPattern(UIA_ScrollPatternId)
                        .and_then(|p| p.cast::<IUIAutomationScrollPattern>())
                        .and_then(|p| p.SetScrollPercent(UIA_ScrollPatternNoScroll, percent))
                };
                common::pause(1500);
                let all = snapshot(&ui);
                let headings: Vec<String> = all
                    .iter()
                    .map(|(_, e)| e)
                    .filter(|e| {
                        // SAFETY: plain UI Automation call.
                        let r = unsafe { e.CurrentBoundingRectangle() }.unwrap_or_default();
                        common::class(e) == "ListViewItem" && r.right - r.left > 1000
                    })
                    .map(|e| format!("{} (off={})", common::name(e), common::offscreen(e)))
                    .collect();
                println!("scroll {percent}%: {result:?}; sections: {headings:?}");
            }
        }
        "flow" => {
            // The whole search as the agent would run it, with timings:
            // open the search box if needed -> SetValue -> wait for the suggestion that equals the
            // query -> invoke it -> wait for "Showing results for" and the first result items.
            let query = args.get(1).expect("usage: flow <query> [library]").clone();
            let library = args.get(2).is_some_and(|a| a == "library");
            let t0 = Instant::now();
            let wait = |what: &str, mut ready: Box<dyn FnMut() -> bool>| -> Option<u128> {
                for _ in 0..150 {
                    if ready() {
                        let ms = t0.elapsed().as_millis();
                        println!("  {what}: {ms} ms");
                        return Some(ms);
                    }
                    common::pause(50);
                }
                println!("  {what}: NOT within 7.5 s");
                None
            };
            let edit_of = |ui: &common::Ui| {
                snapshot(ui).into_iter().map(|(_, e)| e).find(|e| {
                    common::id(e) == "TextBox"
                        && unsafe { e.CurrentControlType() }.map(|t| t.0).unwrap_or(0) == 50004
                        && !common::offscreen(e)
                })
            };
            if edit_of(&ui).is_none() {
                let button = before_all
                    .iter()
                    .find(|(_, e)| common::id(e) == "Search_Button")
                    .expect("no Search_Button")
                    .1
                    .clone();
                println!("opening the search box: {}", common::invoke(&button));
                let ui2 = common::Ui {
                    automation: ui.automation.clone(),
                    window: ui.window.clone(),
                };
                wait(
                    "search box visible",
                    Box::new(move || edit_of(&ui2).is_some()),
                );
            }
            let edit = edit_of(&ui).expect("no search box");
            // SAFETY: plain UI Automation calls.
            unsafe {
                let _ = edit
                    .GetCurrentPattern(UIA_ValuePatternId)
                    .and_then(|p| p.cast::<IUIAutomationValuePattern>())
                    .and_then(|p| p.SetValue(&windows::core::BSTR::from(query.as_str())));
            }
            println!(
                "SetValue({query:?}) done at {} ms",
                t0.elapsed().as_millis()
            );
            let q = query.clone();
            let ui3 = common::Ui {
                automation: ui.automation.clone(),
                window: ui.window.clone(),
            };
            wait(
                "results already live OR the plain-query suggestion visible",
                Box::new(move || {
                    let all = snapshot(&ui3);
                    all.iter().any(|(_, e)| {
                        (common::class(e) == "ListViewItem" && common::name(e) == q)
                            || (common::id(e) == "SearchPageStatus"
                                && common::name(e).to_lowercase().contains(&q.to_lowercase()))
                    })
                }),
            );
            let suggestion = snapshot(&ui)
                .into_iter()
                .map(|(_, e)| e)
                .find(|e| common::class(e) == "ListViewItem" && common::name(e) == query);
            match suggestion {
                Some(s) => println!(
                    "path A (popup): invoke the plain-query suggestion: {}",
                    common::invoke(&s)
                ),
                None => println!("path B (live): the page updated by itself, nothing to invoke"),
            }
            let q = query.clone();
            let ui4 = common::Ui {
                automation: ui.automation.clone(),
                window: ui.window.clone(),
            };
            wait(
                "results page ready (status text + first items)",
                Box::new(move || {
                    let all = snapshot(&ui4);
                    let status = all.iter().any(|(_, e)| {
                        common::id(e) == "SearchPageStatus"
                            && common::name(e).to_lowercase().contains(&q.to_lowercase())
                    });
                    let items = all
                        .iter()
                        .filter(|(_, e)| common::class(e) == "GridViewItem")
                        .count();
                    status && items > 0
                }),
            );
            if library {
                let button = snapshot(&ui)
                    .into_iter()
                    .map(|(_, e)| e)
                    .find(|e| common::id(e) == "SearchLibrary");
                if let Some(button) = button {
                    println!("switch to Your Library: {}", common::toggle(&button));
                    common::pause(1500);
                    println!("  ...and {} ms in total", t0.elapsed().as_millis());
                }
            }
        }
        "toggle" => {
            let id = args.get(1).expect("usage: toggle <AutomationId>");
            let el = before_all
                .iter()
                .find(|(_, e)| common::id(e) == *id)
                .unwrap_or_else(|| panic!("no element with id {id}"))
                .1
                .clone();
            println!(
                "toggle {id} -> {} (before [{}])",
                common::toggle(&el),
                common::patterns(&el)
            );
            common::pause(2500);
            let after = snapshot(&ui);
            if let Some((_, e)) = after.iter().find(|(_, e)| common::id(e) == *id) {
                println!("after [{}]", common::patterns(e));
            }
            print_new(&before, &after, 20);
        }
        "scrollto" => {
            use windows::Win32::UI::Accessibility::{
                IUIAutomationScrollPattern, UIA_ScrollPatternId, UIA_ScrollPatternNoScroll,
            };
            let percent: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let page = before_all
                .iter()
                .map(|(_, e)| e)
                .find(|e| {
                    // SAFETY: plain UI Automation call.
                    let r = unsafe { e.CurrentBoundingRectangle() }.unwrap_or_default();
                    common::class(e) == "ListView"
                        && r.right - r.left > 1000
                        && r.bottom - r.top > 700
                })
                .expect("no page list")
                .clone();
            // SAFETY: plain UI Automation calls.
            let result = unsafe {
                page.GetCurrentPattern(UIA_ScrollPatternId)
                    .and_then(|p| p.cast::<IUIAutomationScrollPattern>())
                    .and_then(|p| p.SetScrollPercent(UIA_ScrollPatternNoScroll, percent))
            };
            println!("scroll to {percent}% -> {result:?}");
            common::pause(1500);
        }
        "props" => {
            // Every descriptive UI Automation property of the first item whose name contains the
            // text: is a link, an id or a kind reachable?
            use windows::Win32::UI::Accessibility::{
                IUIAutomationLegacyIAccessiblePattern, UIA_LegacyIAccessiblePatternId,
            };
            let part = args.get(1).expect("usage: props <text in the name>");
            let el = before_all
                .iter()
                .map(|(_, e)| e)
                .find(|e| {
                    matches!(common::class(e).as_str(), "ListViewItem" | "GridViewItem")
                        && common::name(e).contains(part.as_str())
                })
                .expect("no such item")
                .clone();
            // SAFETY: plain UI Automation calls.
            unsafe {
                println!("name: {:?}", common::name(&el));
                println!(
                    "help_text: {:?}",
                    el.CurrentHelpText().map(|s| s.to_string())
                );
                println!(
                    "item_type: {:?}",
                    el.CurrentItemType().map(|s| s.to_string())
                );
                println!(
                    "item_status: {:?}",
                    el.CurrentItemStatus().map(|s| s.to_string())
                );
                println!(
                    "provider_description: {:?}",
                    el.CurrentProviderDescription().map(|s| s.to_string())
                );
                println!(
                    "aria_properties: {:?}",
                    el.CurrentAriaProperties().map(|s| s.to_string())
                );
                println!(
                    "aria_role: {:?}",
                    el.CurrentAriaRole().map(|s| s.to_string())
                );
                println!(
                    "localized_type: {:?}",
                    el.CurrentLocalizedControlType().map(|s| s.to_string())
                );
                println!(
                    "accelerator_key: {:?}",
                    el.CurrentAcceleratorKey().map(|s| s.to_string())
                );
                println!(
                    "access_key: {:?}",
                    el.CurrentAccessKey().map(|s| s.to_string())
                );
                if let Ok(p) = el
                    .GetCurrentPattern(UIA_LegacyIAccessiblePatternId)
                    .and_then(|p| p.cast::<IUIAutomationLegacyIAccessiblePattern>())
                {
                    println!(
                        "legacy: name={:?} value={:?} description={:?} help={:?} default_action={:?}",
                        p.CurrentName().map(|s| s.to_string()),
                        p.CurrentValue().map(|s| s.to_string()),
                        p.CurrentDescription().map(|s| s.to_string()),
                        p.CurrentHelp().map(|s| s.to_string()),
                        p.CurrentDefaultAction().map(|s| s.to_string())
                    );
                }
            }
        }
        "top" => {
            // Everything above the first result section (scope switches, filters, headings).
            for (_, el) in &before_all {
                // SAFETY: plain UI Automation call.
                let r = unsafe { el.CurrentBoundingRectangle() }.unwrap_or_default();
                if !common::offscreen(el) && r.top < 112 && r.left > 2600 && r.right < 3740 {
                    println!("{}", common::describe(el));
                }
            }
        }
        "back" => {
            let el = before_all
                .iter()
                .find(|(_, e)| common::id(e) == "NavigationViewBackButton")
                .expect("no back button")
                .1
                .clone();
            println!("Back invoke -> {}", common::invoke(&el));
            common::pause(1500);
        }
        other => println!("unknown action {other}"),
    }
}
