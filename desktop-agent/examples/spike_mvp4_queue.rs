//! MVP 4 spike, B: the play-queue pane ("Playing Next" / "History" / "AutoPlay") through UI Automation.
//!   cargo run --example spike_mvp4_queue -- read          read every row (read only)
//!   cargo run --example spike_mvp4_queue -- toggle-pane   close and reopen the pane, timing it
//!   cargo run --example spike_mvp4_queue -- scroll        read, scroll the list by pattern, read again
//!   cargo run --example spike_mvp4_queue -- patterns <n>  list what row n (0-based) supports (changes nothing)

#[path = "spike_common/mod.rs"]
mod common;

use std::time::Instant;

use windows::core::Interface;
use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, IUIAutomationScrollItemPattern, IUIAutomationScrollPattern,
    UIA_ScrollItemPatternId, UIA_ScrollPatternId,
};

const PANE_ID: &str = "PaneRoot";
const QUEUE_BUTTON_ID: &str = "PlayQueueToggleButton";

fn find(ui: &common::Ui, id: &str) -> Option<IUIAutomationElement> {
    common::array_items(&ui.all())
        .into_iter()
        .find(|e| common::id(e) == id)
}

/// The pane and everything under it, in tree order.
fn pane(ui: &common::Ui) -> Option<(IUIAutomationElement, Vec<IUIAutomationElement>)> {
    // Two SplitViewPanes are called "PaneRoot": the left navigation sidebar and the right queue
    // pane. The queue pane is the right-most one.
    let pane = common::array_items(&ui.all())
        .into_iter()
        .filter(|e| common::id(e) == PANE_ID)
        .max_by_key(|e| {
            unsafe { e.CurrentBoundingRectangle() }
                .map(|r| r.left)
                .unwrap_or(0)
        })?;
    let below = common::array_items(&ui.below(&pane));
    Some((pane, below))
}

/// Prints the legacy (MSAA) view of an element: role, state and its default action.
fn legacy(el: &IUIAutomationElement, label: &str) {
    use windows::Win32::UI::Accessibility::{
        IUIAutomationLegacyIAccessiblePattern, UIA_LegacyIAccessiblePatternId,
    };
    // SAFETY: plain UI Automation calls.
    unsafe {
        match el
            .GetCurrentPattern(UIA_LegacyIAccessiblePatternId)
            .and_then(|p| p.cast::<IUIAutomationLegacyIAccessiblePattern>())
        {
            Ok(p) => println!(
                "{label}: legacy role={:?} state={:?} default_action={:?}",
                p.CurrentRole(),
                p.CurrentState(),
                p.CurrentDefaultAction().map(|s| s.to_string())
            ),
            Err(_) => println!("{label}: no legacy pattern"),
        }
    }
}

fn rows(below: &[IUIAutomationElement]) -> Vec<&IUIAutomationElement> {
    below
        .iter()
        .filter(|e| common::class(e) == "ListViewItem")
        .collect()
}

fn print_pane(ui: &common::Ui) {
    let started = Instant::now();
    let Some((pane, below)) = pane(ui) else {
        println!("no PaneRoot in the window (pane closed?)");
        return;
    };
    println!(
        "PaneRoot found: {} ({} descendants, read in {} ms)",
        common::describe(&pane),
        below.len(),
        started.elapsed().as_millis()
    );
    let mut row_rect = (0, 0, 0, 0);
    let mut index = 0;
    for el in &below {
        let r = unsafe { el.CurrentBoundingRectangle() }.unwrap_or_default();
        let inside_row = r.left >= row_rect.0
            && r.right <= row_rect.2
            && r.top >= row_rect.1
            && r.bottom <= row_rect.3
            && row_rect.2 > 0;
        if common::class(el) == "ListViewItem" {
            row_rect = (r.left, r.top, r.right, r.bottom);
            println!(
                "  ROW {index}: name={:?} off={} y={}..{} [{}]",
                common::truncate(&common::name(el), 110),
                common::offscreen(el),
                r.top,
                r.bottom,
                common::patterns(el)
            );
            index += 1;
        } else if inside_row {
            // the row's own parts are shown once, for the first row only
            if index == 1 {
                println!("      part: {}", common::describe(el));
            }
        } else {
            println!("  {}", common::describe(el));
        }
    }
}

fn main() {
    let action = std::env::args().nth(1).unwrap_or_else(|| "read".into());
    let ui = common::start();
    match action.as_str() {
        "read" => print_pane(&ui),
        "next" => {
            // Skips n tracks through SMTC and prints how the queue lists change after each skip.
            let n: usize = std::env::args()
                .nth(2)
                .and_then(|s| s.parse().ok())
                .unwrap_or(3);
            let smtc = am_remote_agent::media::smtc::Smtc::new(
                std::sync::Arc::new(|| {}),
                std::sync::Arc::new(|| {}),
            )
            .expect("SMTC");
            for step in 1..=n {
                let ok = smtc.send(am_remote_agent::media::Transport::Next);
                common::pause(1800);
                let (_, below) = pane(&ui).expect("no queue pane");
                let rows_of = |list_id: &str| {
                    below
                        .iter()
                        .find(|e| common::id(e) == list_id)
                        .map(|l| {
                            common::array_items(&ui.below(l))
                                .iter()
                                .filter(|e| common::class(e) == "ListViewItem")
                                .count()
                        })
                        .unwrap_or(0)
                };
                println!(
                    "after skip {step} (accepted={ok:?}): Playing Next rows={} AutoPlay rows={}",
                    rows_of("PlayQueueListView"),
                    rows_of("PlayQueueListViewAutoplay")
                );
            }
        }
        "summary" => {
            // Per list: how many rows UI Automation exposes, and how much of the scroller is in view.
            let started = Instant::now();
            let (_, below) = pane(&ui).expect("no queue pane");
            println!(
                "read {} elements in {} ms",
                below.len(),
                started.elapsed().as_millis()
            );
            for el in below.iter().filter(|e| common::class(e) == "ListView") {
                let n = common::array_items(&ui.below(el))
                    .iter()
                    .filter(|e| common::class(e) == "ListViewItem")
                    .count();
                println!("list id={:?}: {n} rows exposed", common::id(el));
            }
            if let Some(viewer) = below.iter().find(|e| common::class(e) == "ScrollViewer") {
                // SAFETY: plain UI Automation calls.
                unsafe {
                    if let Ok(p) = viewer
                        .GetCurrentPattern(UIA_ScrollPatternId)
                        .and_then(|p| p.cast::<IUIAutomationScrollPattern>())
                    {
                        println!(
                            "scroller: scrollable={:?} view size={:?}% position={:?}%",
                            p.CurrentVerticallyScrollable().map(|b| b.as_bool()),
                            p.CurrentVerticalViewSize(),
                            p.CurrentVerticalScrollPercent()
                        );
                    }
                }
            }
        }
        "toggle-pane" => {
            let button = find(&ui, QUEUE_BUTTON_ID).expect("no queue button");
            let original = common::patterns(&button);
            println!("queue button at start: {original}");
            print_pane(&ui);
            for step in 1..=2 {
                let started = Instant::now();
                let ok = common::toggle(&find(&ui, QUEUE_BUTTON_ID).unwrap());
                // Poll until the pane's presence/offscreen state settles.
                let mut ms = 0;
                for _ in 0..40 {
                    common::pause(100);
                    ms += 100;
                    let present = pane(&ui).is_some_and(|(_, b)| !rows(&b).is_empty());
                    if present == (step == 2) {
                        break;
                    }
                }
                println!(
                    "toggle #{step} accepted={ok}; button now [{}]; queue rows readable={} after ~{ms} ms of polling (call+poll {} ms)",
                    common::patterns(&find(&ui, QUEUE_BUTTON_ID).unwrap()),
                    pane(&ui).is_some_and(|(_, b)| !rows(&b).is_empty()),
                    started.elapsed().as_millis()
                );
                if step == 1 {
                    println!(
                        "  while closed, rows readable: {}",
                        pane(&ui).map(|(_, b)| rows(&b).len()).unwrap_or(0)
                    );
                }
            }
            common::pause(800);
            print_pane(&ui);
        }
        "scroll" => {
            print_pane(&ui);
            let (_, below) = pane(&ui).expect("no pane");
            let viewer = below
                .iter()
                .find(|e| common::class(e) == "ScrollViewer")
                .expect("no ScrollViewer in the pane");
            // SAFETY: plain UI Automation calls.
            unsafe {
                match viewer
                    .GetCurrentPattern(UIA_ScrollPatternId)
                    .and_then(|p| p.cast::<IUIAutomationScrollPattern>())
                {
                    Ok(p) => println!(
                        "ScrollPattern: vertically scrollable={:?} percent={:?} view size={:?}",
                        p.CurrentVerticallyScrollable().map(|b| b.as_bool()),
                        p.CurrentVerticalScrollPercent(),
                        p.CurrentVerticalViewSize()
                    ),
                    Err(e) => println!("ScrollViewer has no ScrollPattern: {e}"),
                }
                let all_rows = rows(&below);
                if let Some(last) = all_rows.last() {
                    println!(
                        "scrolling the last row into view via ScrollItemPattern: {:?}",
                        last.GetCurrentPattern(UIA_ScrollItemPatternId)
                            .and_then(|p| p.cast::<IUIAutomationScrollItemPattern>())
                            .and_then(|p| p.ScrollIntoView())
                    );
                }
            }
            common::pause(800);
            print_pane(&ui);
        }
        "play" => {
            // Plays queue row n through the legacy default action ("Double Click"), after scrolling
            // it into view, and shows what is playing and what is left in the queue.
            use windows::Win32::UI::Accessibility::{
                IUIAutomationLegacyIAccessiblePattern, UIA_LegacyIAccessiblePatternId,
            };
            let n: usize = std::env::args()
                .nth(2)
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
            let smtc = am_remote_agent::media::smtc::Smtc::new(
                std::sync::Arc::new(|| {}),
                std::sync::Arc::new(|| {}),
            )
            .expect("SMTC");
            let current = |smtc: &am_remote_agent::media::smtc::Smtc| {
                smtc.snapshot()
                    .ok()
                    .flatten()
                    .map(|s| s.to_state(0, 0).title)
                    .unwrap_or_default()
            };
            let names = |ui: &common::Ui| -> Vec<String> {
                let (_, below) = pane(ui).expect("no queue pane");
                rows(&below).iter().map(|e| common::name(e)).collect()
            };
            let before = names(&ui);
            println!("playing now: {:?}", current(&smtc));
            println!(
                "queue before: {} rows; row {n} = {:?}",
                before.len(),
                before.get(n)
            );
            let (_, below) = pane(&ui).expect("no queue pane");
            let all_rows = rows(&below);
            let row = all_rows.get(n).expect("no such row");
            // SAFETY: plain UI Automation calls.
            let started = Instant::now();
            unsafe {
                let _ = row
                    .GetCurrentPattern(UIA_ScrollItemPatternId)
                    .and_then(|p| p.cast::<IUIAutomationScrollItemPattern>())
                    .and_then(|p| p.ScrollIntoView());
                // Method (third argument): legacy | focus-legacy | select | select-legacy |
                // swipe-legacy | legacy-select (MSAA Select with take-focus+take-selection).
                let method = std::env::args().nth(3).unwrap_or_else(|| "legacy".into());
                let legacy_of = |el: &IUIAutomationElement| {
                    el.GetCurrentPattern(UIA_LegacyIAccessiblePatternId)
                        .and_then(|p| p.cast::<IUIAutomationLegacyIAccessiblePattern>())
                };
                let result = match method.as_str() {
                    "legacy" => legacy_of(row).and_then(|p| p.DoDefaultAction()),
                    "focus-legacy" => row
                        .SetFocus()
                        .and_then(|_| legacy_of(row))
                        .and_then(|p| p.DoDefaultAction()),
                    "select" => row
                        .GetCurrentPattern(windows::Win32::UI::Accessibility::UIA_SelectionItemPatternId)
                        .and_then(|p| p.cast::<windows::Win32::UI::Accessibility::IUIAutomationSelectionItemPattern>())
                        .and_then(|p| p.Select()),
                    "select-legacy" => row
                        .GetCurrentPattern(windows::Win32::UI::Accessibility::UIA_SelectionItemPatternId)
                        .and_then(|p| p.cast::<windows::Win32::UI::Accessibility::IUIAutomationSelectionItemPattern>())
                        .and_then(|p| p.Select())
                        .and_then(|_| legacy_of(row))
                        .and_then(|p| p.DoDefaultAction()),
                    "swipe-legacy" => {
                        let child = ui
                            .children(row)
                            .GetElement(0)
                            .expect("row has no child");
                        legacy_of(&child).and_then(|p| p.DoDefaultAction())
                    }
                    "legacy-select" => legacy_of(row).and_then(|p| p.Select(3)),
                    other => panic!("unknown method {other}"),
                };
                println!("method {method} -> {result:?}");
            }
            // Wait until the current song changes (max 6 s).
            let was = current(&smtc);
            let mut after_ms = None;
            for _ in 0..60 {
                common::pause(100);
                if current(&smtc) != was {
                    after_ms = Some(started.elapsed().as_millis());
                    break;
                }
            }
            common::pause(1200);
            let after = names(&ui);
            println!(
                "song changed after {after_ms:?} ms; playing now: {:?}",
                current(&smtc)
            );
            println!("queue after: {} rows", after.len());
            println!("  first 3 rows after : {:?}", &after[..after.len().min(3)]);
            println!(
                "  old rows {}..{}+3  : {:?}",
                n + 1,
                n + 1,
                before.iter().skip(n + 1).take(3).collect::<Vec<_>>()
            );
        }
        "skip-to" => {
            // Jumps to queue row n by sending SMTC Next (n + 1) times with a short gap, then
            // checks which song is playing. Third argument: gap in ms (default 300).
            let n: usize = std::env::args()
                .nth(2)
                .and_then(|s| s.parse().ok())
                .unwrap_or(2);
            let gap: u64 = std::env::args()
                .nth(3)
                .and_then(|s| s.parse().ok())
                .unwrap_or(300);
            let smtc = am_remote_agent::media::smtc::Smtc::new(
                std::sync::Arc::new(|| {}),
                std::sync::Arc::new(|| {}),
            )
            .expect("SMTC");
            let title_of_row = |ui: &common::Ui, i: usize| -> String {
                let (_, below) = pane(ui).expect("no queue pane");
                let all_rows = rows(&below);
                let row = all_rows.get(i).expect("no such row");
                common::array_items(&ui.below(row))
                    .iter()
                    .find(|e| common::class(e) == "TextBlock")
                    .map(common::name)
                    .unwrap_or_default()
            };
            let expected = title_of_row(&ui, n);
            let following = title_of_row(&ui, n + 1);
            let current = |smtc: &am_remote_agent::media::smtc::Smtc| {
                smtc.snapshot()
                    .ok()
                    .flatten()
                    .map(|s| s.to_state(0, 0).title)
                    .unwrap_or_default()
            };
            println!(
                "playing now: {:?}; target row {n}: {expected:?}",
                current(&smtc)
            );
            let started = Instant::now();
            let mut seen = vec![];
            for _ in 0..=n {
                let _ = smtc.send(am_remote_agent::media::Transport::Next);
                common::pause(gap);
                seen.push(current(&smtc));
            }
            // Wait for it to settle.
            let mut settled = None;
            for _ in 0..50 {
                if current(&smtc) == expected {
                    settled = Some(started.elapsed().as_millis());
                    break;
                }
                common::pause(100);
            }
            println!("titles seen after each skip: {seen:?}");
            println!(
                "landed on the target: {} after {settled:?} ms; now playing {:?}",
                settled.is_some(),
                current(&smtc)
            );
            common::pause(1500);
            println!(
                "first row of the queue now: {:?} (expected {following:?})",
                title_of_row(&ui, 0)
            );
        }
        "shuffle-order" => {
            // Reads the queue, turns shuffle on, reads again, turns it off, reads again.
            let titles = |ui: &common::Ui| -> Vec<String> {
                let (_, below) = pane(ui).expect("no queue pane");
                rows(&below)
                    .iter()
                    .map(|row| {
                        common::array_items(&ui.below(row))
                            .iter()
                            .find(|e| common::class(e) == "TextBlock")
                            .map(common::name)
                            .unwrap_or_default()
                    })
                    .collect()
            };
            let button = || find(&ui, "ShuffleButton").expect("no shuffle button");
            let start_state = common::patterns(&button());
            let a = titles(&ui);
            common::toggle(&button());
            common::pause(2000);
            let b = titles(&ui);
            common::toggle(&button());
            common::pause(2000);
            let c = titles(&ui);
            println!(
                "shuffle button at start: {start_state}; now: {}",
                common::patterns(&button())
            );
            println!(
                "rows: before={} shuffled={} after={}",
                a.len(),
                b.len(),
                c.len()
            );
            println!("same order shuffled vs before: {}", a == b);
            println!("same order after switching back vs before: {}", a == c);
            let same_set = {
                let mut x = a.clone();
                let mut y = b.clone();
                x.sort();
                y.sort();
                x == y
            };
            println!("same set of songs when shuffled (window of 20 may differ): {same_set}");
            println!("first 4 before  : {:?}", &a[..a.len().min(4)]);
            println!("first 4 shuffled: {:?}", &b[..b.len().min(4)]);
            println!("first 4 after   : {:?}", &c[..c.len().min(4)]);
        }
        "scroll-end" => {
            // Scrolls the pane to the very end a few times and counts the rows after each wait.
            let count = |ui: &common::Ui| {
                let (_, below) = pane(ui).expect("no queue pane");
                rows(&below).len()
            };
            println!("rows before: {}", count(&ui));
            for round in 1..=4 {
                let (_, below) = pane(&ui).expect("no queue pane");
                let viewer = below
                    .iter()
                    .find(|e| common::class(e) == "ScrollViewer")
                    .expect("no ScrollViewer");
                // SAFETY: plain UI Automation calls.
                unsafe {
                    let ok = viewer
                        .GetCurrentPattern(UIA_ScrollPatternId)
                        .and_then(|p| p.cast::<IUIAutomationScrollPattern>())
                        .and_then(|p| {
                            p.SetScrollPercent(
                                windows::Win32::UI::Accessibility::UIA_ScrollPatternNoScroll,
                                100.0,
                            )
                        });
                    println!("round {round}: SetScrollPercent(100) -> {ok:?}");
                }
                common::pause(2500);
                let (_, below) = pane(&ui).expect("no queue pane");
                let r = rows(&below);
                println!(
                    "  rows now: {}; last row off-screen={}",
                    r.len(),
                    r.last().map(|e| common::offscreen(e)).unwrap_or(true)
                );
            }
        }
        "patterns" => {
            let n: usize = std::env::args()
                .nth(2)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let (_, below) = pane(&ui).expect("no pane");
            let all_rows = rows(&below);
            let row = all_rows.get(n).expect("no such row");
            println!("row {n}: {}", common::describe(row));
            legacy(row, "row");
            for child in common::array_items(&ui.below(row)) {
                println!("   child: {}", common::describe(&child));
                legacy(&child, "   child");
            }
        }
        other => println!("unknown action {other}"),
    }
}
