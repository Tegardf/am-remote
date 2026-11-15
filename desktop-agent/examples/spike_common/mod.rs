//! Shared helpers of the MVP 4 spike probes (`spike_mvp4_*`): find Apple Music's main window with
//! `EnumWindows` (fast; asking UI Automation to list the desktop takes ~10 s) and print elements.
//! Probes only: nothing here is used by the agent.

#![allow(dead_code)]

use windows::core::{Interface, BOOL};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationElementArray,
    IUIAutomationExpandCollapsePattern, IUIAutomationInvokePattern,
    IUIAutomationSelectionItemPattern, IUIAutomationTogglePattern, IUIAutomationValuePattern,
    TreeScope_Children, TreeScope_Descendants, UIA_ExpandCollapsePatternId, UIA_InvokePatternId,
    UIA_ScrollItemPatternId, UIA_SelectionItemPatternId, UIA_TogglePatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible,
};

use am_remote_agent::media::audio::{is_apple_music_path, process_path};

pub const WINDOW_CLASS: &str = "WinUIDesktopWin32WindowClass";

pub struct Ui {
    pub automation: IUIAutomation,
    pub window: IUIAutomationElement,
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam is the Vec passed by `top_level_windows`.
    let windows = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    windows.push(hwnd);
    BOOL(1)
}

fn class_of(hwnd: HWND) -> String {
    let mut buffer = [0u16; 128];
    // SAFETY: the buffer is valid for the length passed.
    let n = unsafe { GetClassNameW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..usize::try_from(n).unwrap_or(0)])
}

/// Starts COM and returns the visible main Apple Music window (class `WINDOW_CLASS`, owned by the
/// Apple Music package). Panics with a clear message otherwise: this is a probe.
pub fn start() -> Ui {
    // SAFETY: first COM call on this thread; plain Win32 / UI Automation calls below.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .expect("COM init");
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).expect("UI Automation");
        let mut all: Vec<HWND> = Vec::new();
        let _ = EnumWindows(Some(collect), LPARAM(&mut all as *mut Vec<HWND> as isize));
        let mut best: Option<(i32, IUIAutomationElement)> = None;
        for hwnd in all {
            if !IsWindowVisible(hwnd).as_bool() || class_of(hwnd) != WINDOW_CLASS {
                continue;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if !process_path(pid).is_some_and(|p| is_apple_music_path(&p)) {
                continue;
            }
            let Ok(el) = automation.ElementFromHandle(hwnd) else {
                continue;
            };
            let Ok(r) = el.CurrentBoundingRectangle() else {
                continue;
            };
            let area = (r.right - r.left) * (r.bottom - r.top);
            if best.as_ref().is_none_or(|(a, _)| area > *a) {
                best = Some((area, el));
            }
        }
        let window = best
            .expect("no visible Apple Music window: open Apple Music and keep it on screen")
            .1;
        Ui { automation, window }
    }
}

impl Ui {
    /// Every descendant of the window (the whole tree, one call).
    pub fn all(&self) -> IUIAutomationElementArray {
        // SAFETY: plain UI Automation calls.
        unsafe {
            let any = self.automation.CreateTrueCondition().expect("condition");
            self.window
                .FindAll(TreeScope_Descendants, &any)
                .expect("FindAll")
        }
    }

    /// Descendants of `el` (one call).
    pub fn below(&self, el: &IUIAutomationElement) -> IUIAutomationElementArray {
        // SAFETY: plain UI Automation calls.
        unsafe {
            let any = self.automation.CreateTrueCondition().expect("condition");
            el.FindAll(TreeScope_Descendants, &any).expect("FindAll")
        }
    }

    /// Direct children of `el`.
    pub fn children(&self, el: &IUIAutomationElement) -> IUIAutomationElementArray {
        // SAFETY: plain UI Automation calls.
        unsafe {
            let any = self.automation.CreateTrueCondition().expect("condition");
            el.FindAll(TreeScope_Children, &any).expect("FindAll")
        }
    }
}

pub fn array_items(array: &IUIAutomationElementArray) -> Vec<IUIAutomationElement> {
    // SAFETY: plain UI Automation calls.
    unsafe {
        (0..array.Length().unwrap_or(0))
            .filter_map(|i| array.GetElement(i).ok())
            .collect()
    }
}

pub fn name(el: &IUIAutomationElement) -> String {
    // SAFETY: plain UI Automation call.
    unsafe { el.CurrentName() }
        .map(|s| s.to_string())
        .unwrap_or_default()
}

pub fn id(el: &IUIAutomationElement) -> String {
    // SAFETY: plain UI Automation call.
    unsafe { el.CurrentAutomationId() }
        .map(|s| s.to_string())
        .unwrap_or_default()
}

pub fn class(el: &IUIAutomationElement) -> String {
    // SAFETY: plain UI Automation call.
    unsafe { el.CurrentClassName() }
        .map(|s| s.to_string())
        .unwrap_or_default()
}

pub fn offscreen(el: &IUIAutomationElement) -> bool {
    // SAFETY: plain UI Automation call.
    unsafe { el.CurrentIsOffscreen() }
        .map(|b| b.as_bool())
        .unwrap_or(true)
}

/// Which of the interesting patterns the element supports, plus toggle state / value when it has them.
pub fn patterns(el: &IUIAutomationElement) -> String {
    let mut out = Vec::new();
    // SAFETY: plain UI Automation calls.
    unsafe {
        if el.GetCurrentPattern(UIA_InvokePatternId).is_ok() {
            out.push("Invoke".to_owned());
        }
        if let Ok(p) = el
            .GetCurrentPattern(UIA_TogglePatternId)
            .and_then(|p| p.cast::<IUIAutomationTogglePattern>())
        {
            out.push(format!(
                "Toggle={}",
                p.CurrentToggleState().map(|s| s.0).unwrap_or(-1)
            ));
        }
        if let Ok(p) = el
            .GetCurrentPattern(UIA_ValuePatternId)
            .and_then(|p| p.cast::<IUIAutomationValuePattern>())
        {
            out.push(format!(
                "Value={:?}",
                p.CurrentValue().map(|s| s.to_string()).unwrap_or_default()
            ));
        }
        if let Ok(p) = el
            .GetCurrentPattern(UIA_SelectionItemPatternId)
            .and_then(|p| p.cast::<IUIAutomationSelectionItemPattern>())
        {
            out.push(format!(
                "SelectionItem(selected={})",
                p.CurrentIsSelected().map(|b| b.as_bool()).unwrap_or(false)
            ));
        }
        if let Ok(p) = el
            .GetCurrentPattern(UIA_ExpandCollapsePatternId)
            .and_then(|p| p.cast::<IUIAutomationExpandCollapsePattern>())
        {
            out.push(format!(
                "ExpandCollapse={}",
                p.CurrentExpandCollapseState().map(|s| s.0).unwrap_or(-1)
            ));
        }
        if el.GetCurrentPattern(UIA_ScrollItemPatternId).is_ok() {
            out.push("ScrollItem".to_owned());
        }
    }
    out.join(",")
}

pub fn describe(el: &IUIAutomationElement) -> String {
    // SAFETY: plain UI Automation calls.
    let (control_type, rect) = unsafe {
        (
            el.CurrentControlType().map(|t| t.0).unwrap_or(0),
            el.CurrentBoundingRectangle().unwrap_or_default(),
        )
    };
    format!(
        "type={control_type} id={:?} name={:?} class={:?} off={} rect=({},{})-({},{}) [{}]",
        id(el),
        truncate(&name(el), 90),
        class(el),
        offscreen(el),
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        patterns(el)
    )
}

pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

/// True when name, id or class contains any of the (lowercase) needles.
pub fn matches_any(el: &IUIAutomationElement, needles: &[&str]) -> bool {
    let haystack = format!("{} {} {}", name(el), id(el), class(el)).to_lowercase();
    needles.iter().any(|n| haystack.contains(n))
}

pub fn invoke(el: &IUIAutomationElement) -> bool {
    // SAFETY: plain UI Automation calls.
    unsafe {
        el.GetCurrentPattern(UIA_InvokePatternId)
            .and_then(|p| p.cast::<IUIAutomationInvokePattern>())
            .and_then(|p| p.Invoke())
            .is_ok()
    }
}

pub fn toggle(el: &IUIAutomationElement) -> bool {
    // SAFETY: plain UI Automation calls.
    unsafe {
        el.GetCurrentPattern(UIA_TogglePatternId)
            .and_then(|p| p.cast::<IUIAutomationTogglePattern>())
            .and_then(|p| p.Toggle())
            .is_ok()
    }
}

pub fn pause(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}
