//! MVP 4 spike, A3/A4: reads and toggles Apple Music's Shuffle and Repeat buttons through UI
//! Automation (TogglePattern / InvokePattern, no input simulation), and counts the SMTC
//! `PlaybackInfoChanged` events each change raises. The original state is restored at the end.
//!   cargo run --example spike_mvp4_uia_shuffle

#[path = "spike_common/mod.rs"]
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use windows::Foundation::TypedEventHandler;
use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;
use windows::Win32::UI::Accessibility::IUIAutomationElement;

fn find(ui: &common::Ui, id: &str) -> Option<IUIAutomationElement> {
    common::array_items(&ui.all())
        .into_iter()
        .find(|e| common::id(e) == id)
}

fn shuffle_state(el: &IUIAutomationElement) -> String {
    format!("{}  [{}]", common::name(el), common::patterns(el))
}

fn main() -> windows::core::Result<()> {
    let ui = common::start();

    // SMTC event counters.
    let info_events = Arc::new(AtomicUsize::new(0));
    let props_events = Arc::new(AtomicUsize::new(0));
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()?;
    let session = manager
        .GetSessions()?
        .into_iter()
        .find(|s| {
            s.SourceAppUserModelId()
                .map(|a| {
                    a.to_string()
                        .to_lowercase()
                        .starts_with("appleinc.applemusicwin")
                })
                .unwrap_or(false)
        })
        .expect("no Apple Music SMTC session");
    let c = info_events.clone();
    let t1 = session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
        c.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }))?;
    let c = props_events.clone();
    let t2 = session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
        c.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }))?;
    common::pause(1500); // let the initial events settle
    let report = |label: &str| {
        println!(
            "  [{label}] PlaybackInfoChanged so far={} MediaPropertiesChanged so far={}",
            info_events.load(Ordering::SeqCst),
            props_events.load(Ordering::SeqCst)
        );
    };

    // ---- Shuffle ----
    let shuffle = find(&ui, "ShuffleButton").expect("ShuffleButton not found");
    let original = shuffle_state(&shuffle);
    println!("shuffle at start: {original}");
    report("start");
    for step in 1..=2 {
        let ok = common::toggle(&shuffle);
        common::pause(1500);
        let now = find(&ui, "ShuffleButton").expect("ShuffleButton vanished");
        println!("toggle #{step} accepted={ok} -> {}", shuffle_state(&now));
        report(&format!("after shuffle toggle {step}"));
    }
    let after = shuffle_state(&find(&ui, "ShuffleButton").unwrap());
    println!("shuffle after two toggles (should equal start): {after}");

    // ---- Repeat ----
    let repeat = find(&ui, "RepeatButton").expect("RepeatButton not found");
    let repeat_original = common::name(&repeat);
    println!(
        "repeat at start: {repeat_original:?} [{}]",
        common::patterns(&repeat)
    );
    let mut seen = vec![repeat_original.clone()];
    for step in 1..=4 {
        let el = find(&ui, "RepeatButton").unwrap();
        let ok = common::invoke(&el);
        common::pause(1200);
        let now = find(&ui, "RepeatButton").unwrap();
        println!(
            "invoke #{step} accepted={ok} -> name={:?} [{}]",
            common::name(&now),
            common::patterns(&now)
        );
        report(&format!("after repeat invoke {step}"));
        seen.push(common::name(&now));
        if common::name(&now) == repeat_original && step >= 2 {
            println!("back at the original after {step} invokes");
            break;
        }
    }
    // Restore (the cycle is at most 3 long).
    for _ in 0..3 {
        let el = find(&ui, "RepeatButton").unwrap();
        if common::name(&el) == repeat_original {
            break;
        }
        common::invoke(&el);
        common::pause(1200);
    }
    println!(
        "repeat states seen: {seen:?}; final: {:?}",
        common::name(&find(&ui, "RepeatButton").unwrap())
    );
    let shuffle_end = shuffle_state(&find(&ui, "ShuffleButton").unwrap());
    println!("shuffle at end: {shuffle_end} (started: {original})");

    session.RemovePlaybackInfoChanged(t1)?;
    session.RemoveMediaPropertiesChanged(t2)?;
    Ok(())
}
