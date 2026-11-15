//! MVP 4 spike, B5: does a queue change raise a UI Automation event? Subscribes to structure-changed
//! and property-changed events on the queue pane, skips one track through SMTC, and counts events.
//!   cargo run --example spike_mvp4_events

#[path = "spike_common/mod.rs"]
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use windows::core::{implement, Ref, Result};
use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, IUIAutomationStructureChangedEventHandler,
    IUIAutomationStructureChangedEventHandler_Impl, StructureChangeType, TreeScope_Subtree,
};

#[implement(IUIAutomationStructureChangedEventHandler)]
struct Counter(Arc<AtomicUsize>);

impl IUIAutomationStructureChangedEventHandler_Impl for Counter_Impl {
    fn HandleStructureChangedEvent(
        &self,
        _sender: Ref<'_, IUIAutomationElement>,
        _change: StructureChangeType,
        _runtime_id: *const windows::Win32::System::Com::SAFEARRAY,
    ) -> Result<()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn main() -> Result<()> {
    let ui = common::start();
    // The right-most PaneRoot is the queue pane.
    let pane = common::array_items(&ui.all())
        .into_iter()
        .filter(|e| common::id(e) == "PaneRoot")
        .max_by_key(|e| {
            unsafe { e.CurrentBoundingRectangle() }
                .map(|r| r.left)
                .unwrap_or(0)
        })
        .expect("no queue pane");
    let count = Arc::new(AtomicUsize::new(0));
    let handler: IUIAutomationStructureChangedEventHandler = Counter(count.clone()).into();
    // SAFETY: plain UI Automation calls; the handler is removed before exit.
    unsafe {
        ui.automation
            .AddStructureChangedEventHandler(&pane, TreeScope_Subtree, None, &handler)?;
    }
    common::pause(2000);
    println!(
        "events while idle for 2 s: {}",
        count.load(Ordering::SeqCst)
    );

    let smtc =
        am_remote_agent::media::smtc::Smtc::new(Arc::new(|| {}), Arc::new(|| {})).expect("SMTC");
    let before = count.load(Ordering::SeqCst);
    let started = Instant::now();
    let _ = smtc.send(am_remote_agent::media::Transport::Next);
    let mut first = None;
    for _ in 0..40 {
        common::pause(100);
        if first.is_none() && count.load(Ordering::SeqCst) > before {
            first = Some(started.elapsed().as_millis());
        }
    }
    println!(
        "after one skip: {} structure events, the first after {first:?} ms",
        count.load(Ordering::SeqCst) - before
    );
    // SAFETY: removes what was added above.
    unsafe {
        ui.automation
            .RemoveStructureChangedEventHandler(&pane, &handler)?
    };
    Ok(())
}
