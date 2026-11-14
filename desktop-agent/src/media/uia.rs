//! UI Automation of the Apple Music window: library playlists, playing a playlist, playing a song.
//!
//! This is the fragile part (see `docs/spikes/link-playback.md`): it drives Apple Music's own UI
//! through the public Windows accessibility API, so an app update can break it. Every step has a
//! timeout and a named failure, and nothing here ever blocks the media thread: it runs on its own
//! thread (with its own COM apartment) and answers over a oneshot channel.
//!
//! Nothing is injected, no keyboard or mouse input is simulated, and Apple Music's files are not
//! touched. Only the Apple Music process's windows are ever acted on.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use tokio::sync::oneshot;
use windows::core::{Interface, BOOL, HSTRING};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern,
    IUIAutomationScrollItemPattern, IUIAutomationScrollPattern, IUIAutomationSelectionItemPattern,
    ScrollAmount_LargeIncrement, ScrollAmount_NoAmount, TreeScope_Descendants,
    UIA_AutomationIdPropertyId, UIA_ButtonControlTypeId, UIA_ControlTypePropertyId,
    UIA_InvokePatternId, UIA_IsScrollPatternAvailablePropertyId, UIA_ListItemControlTypeId,
    UIA_MenuItemControlTypeId, UIA_NamePropertyId, UIA_ScrollItemPatternId, UIA_ScrollPatternId,
    UIA_SelectionItemPatternId, UIA_TextControlTypeId,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowThreadProcessId, ShowWindow, SW_RESTORE, SW_SHOWNORMAL,
};

use super::audio::{is_apple_music_path, process_path};
use super::library::{
    find_track_row, is_page_row_name, menu_item_plays, playlist_from_sidebar_item, same_text,
    Playlist, PAGE_HEADER_ID, PLAYLISTS_HEADER_ID, PLAY_BUTTON_ID,
};
use super::MediaError;

/// How long a handler waits for the whole UI operation before answering `INTERNAL`.
const OP_TIMEOUT: Duration = Duration::from_secs(35);
const POLL: Duration = Duration::from_millis(150);
/// A page (album or playlist) must open and show its header within this time.
const PAGE_WAIT: Duration = Duration::from_secs(10);
/// The context menu of a track row must show up within this time.
const MENU_WAIT: Duration = Duration::from_secs(3);
/// The sidebar must list the playlists within this time after selecting "Playlists".
const SIDEBAR_WAIT: Duration = Duration::from_secs(3);
/// A hidden window must become visible within this time after asking to restore it.
const RESTORE_WAIT: Duration = Duration::from_secs(3);
/// How far down an album page is scrolled to find a song row before giving up.
const MAX_SCROLLS: usize = 15;
/// How long dropping the driver waits for its thread.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(2);

/// Class name of Apple Music's WinUI windows (main window and mini player).
// verified: spike_playlist_page, 2026-11-14
const WINDOW_CLASS: &str = "WinUIDesktopWin32WindowClass";
/// AutomationId of the scroll viewer that holds the sidebar entries.
// verified: spike_playlists, 2026-11-14
const SIDEBAR_SCROLLER_ID: &str = "MenuItemsScrollViewer";
/// AutomationId of the main window's navigation view; the mini player does not have it.
// verified: spike_playlists, 2026-11-14
const NAV_VIEW_ID: &str = "NavView";

/// What kind of page the content area is expected to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageKind {
    Playlist,
    Album,
}

/// AutomationId of the transport bar's Play/Pause button, at the top of the window.
// verified: spike_playlist_page, 2026-11-14
const TRANSPORT_PLAY_ID: &str = "TransportControl_PlayPauseStop";

/// Why a UI operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiaError {
    /// There is no Apple Music window.
    NotRunning,
    /// The thing asked for is not there (an unknown playlist id).
    NotFound(String),
    /// Apple Music's UI did not behave as expected: which step failed.
    Failed(String),
}

impl From<UiaError> for MediaError {
    fn from(error: UiaError) -> Self {
        match error {
            UiaError::NotRunning => Self::NotRunning,
            UiaError::NotFound(message) => Self::NotFound(message),
            UiaError::Failed(message) => Self::NotSupported(message),
        }
    }
}

fn failed(step: &str, detail: impl std::fmt::Display) -> UiaError {
    UiaError::Failed(format!("{step} ({detail})"))
}

/// Polls `check` until it gives a value or `timeout` passes.
fn wait_for<T>(timeout: Duration, mut check: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = check() {
            return Some(value);
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(POLL);
    }
}

/// Everything that touches the UI. Lives on the UI thread only.
struct Driver {
    automation: IUIAutomation,
}

impl Driver {
    // ---------- finding things ----------

    fn condition(
        &self,
        property: windows::Win32::UI::Accessibility::UIA_PROPERTY_ID,
        value: &VARIANT,
    ) -> Result<windows::Win32::UI::Accessibility::IUIAutomationCondition, UiaError> {
        // SAFETY: plain UI Automation call.
        unsafe { self.automation.CreatePropertyCondition(property, value) }
            .map_err(|e| failed("cannot build a UI query", e))
    }

    fn find_all(
        &self,
        root: &IUIAutomationElement,
        property: windows::Win32::UI::Accessibility::UIA_PROPERTY_ID,
        value: &VARIANT,
    ) -> Result<Vec<IUIAutomationElement>, UiaError> {
        let condition = self.condition(property, value)?;
        // SAFETY: plain UI Automation calls on elements we hold.
        unsafe {
            let found = root
                .FindAll(TreeScope_Descendants, &condition)
                .map_err(|e| failed("cannot search the window", e))?;
            let count = found.Length().unwrap_or(0);
            Ok((0..count)
                .filter_map(|i| found.GetElement(i).ok())
                .collect())
        }
    }

    fn find_by_id(&self, root: &IUIAutomationElement, id: &str) -> Option<IUIAutomationElement> {
        let condition = self
            .condition(UIA_AutomationIdPropertyId, &VARIANT::from(id))
            .ok()?;
        // SAFETY: plain UI Automation call.
        unsafe { root.FindFirst(TreeScope_Descendants, &condition) }.ok()
    }

    /// Every top-level window of the Apple Music app (any class, or only `class`) as UI Automation
    /// elements. Found with plain Win32 `EnumWindows`: asking UI Automation itself to list the
    /// desktop's windows takes about 10 s here, because it queries every app's accessibility provider.
    fn apple_music_windows(&self, class: Option<&str>) -> Vec<IUIAutomationElement> {
        let mut is_apple: std::collections::HashMap<u32, bool> = std::collections::HashMap::new();
        top_level_windows()
            .into_iter()
            .filter_map(|hwnd| {
                let mut pid = 0u32;
                // SAFETY: plain Win32 call with a valid out pointer.
                unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
                let ours = *is_apple.entry(pid).or_insert_with(|| {
                    process_path(pid).is_some_and(|path| is_apple_music_path(&path))
                });
                if !ours || class.is_some_and(|wanted| window_class(hwnd) != wanted) {
                    return None; // another app: never touched
                }
                // SAFETY: plain UI Automation call.
                unsafe { self.automation.ElementFromHandle(hwnd) }.ok()
            })
            .collect()
    }

    /// Apple Music's main window (the one with the sidebar). A minimized window is restored.
    fn main_window(&self) -> Result<IUIAutomationElement, UiaError> {
        // The visible windows first: looking inside the hidden mini-player window for the sidebar
        // walks hundreds of elements, so hidden windows are only examined when no visible main
        // window exists.
        let mut visible: Vec<IUIAutomationElement> = Vec::new();
        let mut hidden: Vec<IUIAutomationElement> = Vec::new();
        for window in self.apple_music_windows(Some(WINDOW_CLASS)) {
            // SAFETY: plain UI Automation call.
            let offscreen = unsafe { window.CurrentIsOffscreen() }
                .map(|b| b.as_bool())
                .unwrap_or(true);
            if offscreen {
                hidden.push(window);
            } else {
                visible.push(window);
            }
        }
        // The main window has the sidebar; the mini player does not.
        if let Some(window) = visible
            .into_iter()
            .find(|w| self.find_by_id(w, NAV_VIEW_ID).is_some())
        {
            return Ok(window);
        }
        let window = hidden
            .into_iter()
            .find(|w| self.find_by_id(w, NAV_VIEW_ID).is_some())
            .ok_or(UiaError::NotRunning)?;
        // SAFETY: the handle comes from the window just found; SW_RESTORE only un-minimizes it.
        unsafe {
            if let Ok(handle) = window.CurrentNativeWindowHandle() {
                let _ = ShowWindow(handle, SW_RESTORE);
            }
        }
        wait_for(RESTORE_WAIT, || {
            // SAFETY: plain UI Automation call.
            let offscreen = unsafe { window.CurrentIsOffscreen() }.map(|b| b.as_bool());
            (offscreen == Ok(false)).then_some(())
        })
        .ok_or_else(|| {
            UiaError::Failed("Apple Music's window is minimized and could not be restored".into())
        })?;
        Ok(window)
    }

    // ---------- acting on things ----------

    fn select(&self, element: &IUIAutomationElement) -> Result<(), UiaError> {
        // SAFETY: plain UI Automation calls.
        unsafe {
            element
                .GetCurrentPattern(UIA_SelectionItemPatternId)
                .and_then(|p| p.cast::<IUIAutomationSelectionItemPattern>())
                .and_then(|p| p.Select())
        }
        .map_err(|e| failed("cannot select the sidebar entry", e))
    }

    fn invoke(&self, element: &IUIAutomationElement, what: &str) -> Result<(), UiaError> {
        // SAFETY: plain UI Automation calls.
        unsafe {
            element
                .GetCurrentPattern(UIA_InvokePatternId)
                .and_then(|p| p.cast::<IUIAutomationInvokePattern>())
                .and_then(|p| p.Invoke())
        }
        .map_err(|e| failed(&format!("cannot press {what}"), e))
    }

    /// Best effort: scrolls a realised element into view.
    fn bring_into_view(&self, element: &IUIAutomationElement) {
        // SAFETY: plain UI Automation calls; failure just means it was already visible.
        unsafe {
            let _ = element
                .GetCurrentPattern(UIA_ScrollItemPatternId)
                .and_then(|p| p.cast::<IUIAutomationScrollItemPattern>())
                .and_then(|p| p.ScrollIntoView());
        }
    }

    fn name_of(&self, element: &IUIAutomationElement) -> String {
        // SAFETY: plain UI Automation call.
        unsafe { element.CurrentName() }
            .map(|n| n.to_string())
            .unwrap_or_default()
    }

    // ---------- playlists ----------

    /// The library playlists the sidebar shows right now, with their elements.
    ///
    /// UNVERIFIED: only playlists the sidebar has realised are seen; a very long library might need
    /// the sidebar scrolled (not done), and playlist folders are not handled.
    fn sidebar_playlists(
        &self,
        window: &IUIAutomationElement,
    ) -> Result<Vec<(Playlist, IUIAutomationElement)>, UiaError> {
        // Searching the whole window is slow (the page and the queue pane hold hundreds of
        // elements), so look in the sidebar first and only fall back to everything.
        if let Some(sidebar) = self.find_by_id(window, SIDEBAR_SCROLLER_ID) {
            let found = self.playlists_under(&sidebar)?;
            if !found.is_empty() {
                return Ok(found);
            }
        }
        self.playlists_under(window)
    }

    /// The library playlists among the list items below `root`.
    fn playlists_under(
        &self,
        root: &IUIAutomationElement,
    ) -> Result<Vec<(Playlist, IUIAutomationElement)>, UiaError> {
        let items = self.find_all(
            root,
            UIA_ControlTypePropertyId,
            &VARIANT::from(UIA_ListItemControlTypeId.0),
        )?;
        let mut out: Vec<(Playlist, IUIAutomationElement)> = Vec::new();
        for item in items {
            // SAFETY: plain UI Automation call.
            let id = unsafe { item.CurrentAutomationId() }
                .map(|s| s.to_string())
                .unwrap_or_default();
            if let Some(playlist) = playlist_from_sidebar_item(&id, &self.name_of(&item)) {
                if !out.iter().any(|(known, _)| known.id == playlist.id) {
                    out.push((playlist, item));
                }
            }
        }
        Ok(out)
    }

    /// The library playlists, expanding the sidebar's Playlists list if it is not showing yet.
    ///
    /// Selecting the "Playlists" entry **toggles** the list (selecting it while it is open closes
    /// it), so the entry is only selected when no playlist is visible. After selecting, the result is
    /// returned once the list has stopped growing.
    fn open_playlists(
        &self,
        window: &IUIAutomationElement,
    ) -> Result<Vec<(Playlist, IUIAutomationElement)>, UiaError> {
        let already_open = self.sidebar_playlists(window)?;
        if !already_open.is_empty() {
            return Ok(already_open);
        }
        let header = self
            .find_by_id(window, PLAYLISTS_HEADER_ID)
            .ok_or_else(|| {
                UiaError::Failed("the Apple Music sidebar has no Playlists entry".into())
            })?;
        self.select(&header)?;

        let deadline = Instant::now() + SIDEBAR_WAIT;
        let mut last = self.sidebar_playlists(window)?;
        let mut stable = 0;
        while Instant::now() < deadline && stable < 2 {
            thread::sleep(POLL);
            let now = self.sidebar_playlists(window)?;
            stable = if now.len() == last.len() && !now.is_empty() {
                stable + 1
            } else {
                0
            };
            last = now;
        }
        Ok(last)
    }

    fn playlists(&self) -> Result<Vec<Playlist>, UiaError> {
        let window = self.main_window()?;
        Ok(self
            .open_playlists(&window)?
            .into_iter()
            .map(|(playlist, _)| playlist)
            .collect())
    }

    fn play_playlist(&self, id: &str) -> Result<(), UiaError> {
        let window = self.main_window()?;
        let (playlist, item) = self
            .open_playlists(&window)?
            .into_iter()
            .find(|(playlist, _)| playlist.id == id)
            .ok_or_else(|| UiaError::NotFound("There is no playlist with this id".into()))?;
        self.bring_into_view(&item);
        self.select(&item)?;
        self.wait_for_page(&window, &playlist.name, PageKind::Playlist)?;
        self.press_page_play(&window)
    }

    // ---------- pages ----------

    /// Does the content area show the page called `name`?
    ///
    /// * A playlist page has a header element (`PAGE_HEADER_ID`) named exactly like the playlist.
    /// * An album page has no such header; its name is one visible text element with exactly that
    ///   name, below the transport bar (the same words also appear up there as the title of the
    ///   playing song, which must not count).
    // verified: spike_playlist_page (playlists) and uia_named (albums), 2026-11-14
    fn page_shows(&self, window: &IUIAutomationElement, name: &str, kind: PageKind) -> bool {
        match kind {
            PageKind::Playlist => self
                .find_by_id(window, PAGE_HEADER_ID)
                .is_some_and(|header| same_text(&self.name_of(&header), name)),
            PageKind::Album => {
                // The transport controls sit at the top of the window; the page starts below them.
                // SAFETY: plain UI Automation call.
                let content_top = self
                    .find_by_id(window, TRANSPORT_PLAY_ID)
                    .and_then(|button| unsafe { button.CurrentBoundingRectangle() }.ok())
                    .map_or(0, |rect| rect.bottom);
                let Ok(named) = self.find_all(window, UIA_NamePropertyId, &VARIANT::from(name))
                else {
                    return false;
                };
                named.iter().any(|element| {
                    // SAFETY: plain UI Automation calls.
                    unsafe {
                        element.CurrentControlType() == Ok(UIA_TextControlTypeId)
                            && element
                                .CurrentIsOffscreen()
                                .map(|b| !b.as_bool())
                                .unwrap_or(false)
                            && element
                                .CurrentBoundingRectangle()
                                .is_ok_and(|rect| rect.top >= content_top)
                    }
                })
            }
        }
    }

    /// Waits until the content page is the one called `name` (see [`Driver::page_shows`]).
    fn wait_for_page(
        &self,
        window: &IUIAutomationElement,
        name: &str,
        kind: PageKind,
    ) -> Result<(), UiaError> {
        wait_for(PAGE_WAIT, || {
            self.page_shows(window, name, kind).then_some(())
        })
        .ok_or_else(|| {
            UiaError::Failed(format!(
                "the page \"{name}\" did not open in Apple Music in time"
            ))
        })
    }

    fn press_page_play(&self, window: &IUIAutomationElement) -> Result<(), UiaError> {
        let button = wait_for(PAGE_WAIT, || self.find_by_id(window, PLAY_BUTTON_ID))
            .ok_or_else(|| UiaError::Failed("the page has no Play button".into()))?;
        self.invoke(&button, "the page Play button")
    }

    // ---------- songs ----------

    /// The rows of the album page (never the play-queue pane's rows), with their names.
    fn page_rows(&self, window: &IUIAutomationElement) -> Vec<(IUIAutomationElement, String)> {
        self.find_all(
            window,
            UIA_ControlTypePropertyId,
            &VARIANT::from(UIA_ListItemControlTypeId.0),
        )
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| {
            let name = self.name_of(&row);
            is_page_row_name(&name).then_some((row, name))
        })
        .collect()
    }

    /// Scrolls the content page one page down. False if it cannot go further.
    fn scroll_page_down(&self, window: &IUIAutomationElement) -> bool {
        let Ok(scrollers) = self.find_all(
            window,
            UIA_IsScrollPatternAvailablePropertyId,
            &VARIANT::from(true),
        ) else {
            return false;
        };
        // The page is the largest vertically scrollable element (the sidebar and the queue pane are
        // narrower).
        let mut page: Option<(i64, IUIAutomationScrollPattern)> = None;
        for scroller in scrollers {
            // SAFETY: plain UI Automation calls.
            unsafe {
                let Ok(pattern) = scroller
                    .GetCurrentPattern(UIA_ScrollPatternId)
                    .and_then(|p| p.cast::<IUIAutomationScrollPattern>())
                else {
                    continue;
                };
                if !pattern
                    .CurrentVerticallyScrollable()
                    .map(|b| b.as_bool())
                    .unwrap_or(false)
                {
                    continue;
                }
                let Ok(rect) = scroller.CurrentBoundingRectangle() else {
                    continue;
                };
                let area = i64::from(rect.right - rect.left) * i64::from(rect.bottom - rect.top);
                if page.as_ref().is_none_or(|(best, _)| area > *best) {
                    page = Some((area, pattern));
                }
            }
        }
        let Some((_, page)) = page else {
            return false;
        };
        // SAFETY: plain UI Automation calls.
        unsafe {
            if page.CurrentVerticalScrollPercent().unwrap_or(100.0) >= 99.5 {
                return false;
            }
            page.Scroll(ScrollAmount_NoAmount, ScrollAmount_LargeIncrement)
                .is_ok()
        }
    }

    /// Menu items of every Apple Music window (a menu is a popup, so it may be a window of its own).
    fn menu_items(&self) -> Vec<IUIAutomationElement> {
        self.apple_music_windows(None)
            .iter()
            .flat_map(|window| {
                self.find_all(
                    window,
                    UIA_ControlTypePropertyId,
                    &VARIANT::from(UIA_MenuItemControlTypeId.0),
                )
                .unwrap_or_default()
            })
            .collect()
    }

    fn play_track(&self, open_url: &str, title: &str, album: &str) -> Result<(), UiaError> {
        let window = self.main_window()?;

        shell_open(open_url)?;
        // The page must be this song's album: rows of the page that was showing before the link
        // opened must not be mistaken for it (a same-titled song of another album).
        if album.trim().is_empty() {
            return Err(UiaError::Failed(
                "the catalog did not say which album the song is on".into(),
            ));
        }
        self.wait_for_page(&window, album, PageKind::Album)?;

        // The song row: scroll the page down until it shows up.
        let mut scrolls = 0;
        let row = wait_for(PAGE_WAIT, || {
            let rows = self.page_rows(&window);
            let names: Vec<String> = rows.iter().map(|(_, name)| name.clone()).collect();
            if let Some(index) = find_track_row(&names, title) {
                return rows.into_iter().nth(index).map(|(row, _)| row);
            }
            if !rows.is_empty() && scrolls < MAX_SCROLLS && self.scroll_page_down(&window) {
                scrolls += 1;
            }
            None
        })
        .ok_or_else(|| {
            UiaError::Failed(format!(
                "the song \"{title}\" was not found on the album page Apple Music opened"
            ))
        })?;
        self.bring_into_view(&row);

        // The row's own "..." button (the only button inside it) opens the song menu.
        let buttons = self.find_all(
            &row,
            UIA_ControlTypePropertyId,
            &VARIANT::from(UIA_ButtonControlTypeId.0),
        )?;
        let menu_button = buttons
            .into_iter()
            .next()
            .ok_or_else(|| UiaError::Failed("the song row has no menu button".into()))?;
        self.invoke(&menu_button, "the song menu button")?;

        let play = wait_for(MENU_WAIT, || {
            self.menu_items()
                .into_iter()
                .find(|item| menu_item_plays(&self.name_of(item), title))
        });
        match play {
            Some(item) => self.invoke(&item, "Play in the song menu"),
            None => {
                // Close the menu again by toggling its button, so the UI is not left open.
                // UNVERIFIED: that a second press closes the menu (a light-dismiss flyout might
                // reopen); the failure is reported either way.
                let _ = self.invoke(&menu_button, "the song menu button");
                Err(UiaError::Failed(
                    "the song menu has no entry that plays the song".into(),
                ))
            }
        }
    }
}

unsafe extern "system" fn collect_window(hwnd: HWND, list: LPARAM) -> BOOL {
    // SAFETY: `list` is the pointer to the Vec passed to EnumWindows below, alive for the call.
    let windows = unsafe { &mut *(list.0 as *mut Vec<HWND>) };
    windows.push(hwnd);
    BOOL(1)
}

/// Every top-level window of the desktop.
fn top_level_windows() -> Vec<HWND> {
    let mut windows: Vec<HWND> = Vec::new();
    // SAFETY: the callback only pushes to the Vec, which outlives the call.
    unsafe {
        let _ = EnumWindows(
            Some(collect_window),
            LPARAM(&mut windows as *mut Vec<HWND> as isize),
        );
    }
    windows
}

/// The window class name of `hwnd`.
fn window_class(hwnd: HWND) -> String {
    let mut buffer = [0u16; 128];
    // SAFETY: the buffer is valid for the length passed.
    let length = unsafe { GetClassNameW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..usize::try_from(length).unwrap_or(0)])
}

/// Asks Windows to open `url` with the app registered for its scheme (`music://` is Apple Music).
fn shell_open(url: &str) -> Result<(), UiaError> {
    // SAFETY: plain ShellExecuteW call with owned, valid strings.
    let result = unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from(url),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    // ShellExecute reports success with a value above 32.
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(UiaError::Failed(
            "Windows could not open the link in Apple Music".into(),
        ))
    }
}

// ---------- the UI thread and its async face ----------

type Reply<T> = oneshot::Sender<T>;

enum Command {
    Playlists(Reply<Result<Vec<Playlist>, UiaError>>),
    PlayPlaylist(String, Reply<Result<(), UiaError>>),
    PlayTrack {
        open_url: String,
        title: String,
        album: String,
        reply: Reply<Result<(), UiaError>>,
    },
    Stop,
}

/// Handle to the UI thread.
pub struct Uia {
    commands: mpsc::Sender<Command>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Uia {
    /// Starts the UI thread and waits until UI Automation is available on it.
    pub fn start() -> Result<Self> {
        let (commands, inbox) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let thread = thread::Builder::new()
            .name("uia".into())
            .spawn(move || run(inbox, ready_tx))
            .context("cannot start the UI Automation thread")?;
        ready_rx
            .recv()
            .context("the UI Automation thread stopped during startup")?
            .map_err(|e| anyhow!(e))?;
        Ok(Self {
            commands,
            thread: Some(thread),
        })
    }

    async fn ask<T>(
        &self,
        build: impl FnOnce(Reply<Result<T, UiaError>>) -> Command,
    ) -> Result<T, MediaError> {
        let stopped = || MediaError::Internal("the UI Automation thread is not running".into());
        let (reply, answer) = oneshot::channel();
        self.commands.send(build(reply)).map_err(|_| stopped())?;
        match tokio::time::timeout(OP_TIMEOUT, answer).await {
            Ok(answer) => answer.map_err(|_| stopped())?.map_err(MediaError::from),
            Err(_) => Err(MediaError::NotSupported(
                "Apple Music took too long to respond".into(),
            )),
        }
    }

    pub async fn playlists(&self) -> Result<Vec<Playlist>, MediaError> {
        self.ask(Command::Playlists).await
    }

    pub async fn play_playlist(&self, id: &str) -> Result<(), MediaError> {
        let id = id.to_owned();
        self.ask(|reply| Command::PlayPlaylist(id, reply)).await
    }

    pub async fn play_track(
        &self,
        open_url: &str,
        title: &str,
        album: &str,
    ) -> Result<(), MediaError> {
        let (open_url, title, album) = (open_url.to_owned(), title.to_owned(), album.to_owned());
        self.ask(|reply| Command::PlayTrack {
            open_url,
            title,
            album,
            reply,
        })
        .await
    }
}

impl Drop for Uia {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(thread) = self.thread.take() {
            let deadline = Instant::now() + SHUTDOWN_WAIT;
            while !thread.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(20));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}

/// Runs one UI operation; a panic inside it becomes a `Failed` answer instead of killing the UI
/// thread (which would turn every later request into `INTERNAL` until the agent restarts).
fn guarded<T>(operation: impl FnOnce() -> Result<T, UiaError>) -> Result<T, UiaError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)).unwrap_or_else(|_| {
        tracing::error!("the UI Automation driver panicked");
        Err(UiaError::Failed(
            "the UI driver hit an unexpected problem".into(),
        ))
    })
}

fn run(inbox: mpsc::Receiver<Command>, ready: mpsc::Sender<Result<(), String>>) {
    // SAFETY: first COM call on this thread; paired with CoUninitialize below.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if com.is_err() {
        let _ = ready.send(Err(format!("CoInitializeEx failed: {com:?}")));
        return;
    }
    // SAFETY: plain COM call.
    let automation: IUIAutomation =
        match unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) } {
            Ok(automation) => automation,
            Err(e) => {
                let _ = ready.send(Err(format!("UI Automation is not available: {e}")));
                // SAFETY: balances the successful CoInitializeEx above.
                unsafe { CoUninitialize() };
                return;
            }
        };
    let driver = Driver { automation };
    let _ = ready.send(Ok(()));

    // A dropped receiver just means the HTTP request went away; ignore send errors. A request
    // nobody waits for any more (the client left, or the 35 s timeout fired) is skipped: it must
    // not drive Apple Music's window for nobody, and retries must not pile up.
    while let Ok(command) = inbox.recv() {
        match command {
            Command::Stop => break,
            Command::Playlists(reply) => {
                if !reply.is_closed() {
                    let _ = reply.send(guarded(|| driver.playlists()));
                }
            }
            Command::PlayPlaylist(id, reply) => {
                if !reply.is_closed() {
                    let _ = reply.send(guarded(|| driver.play_playlist(&id)));
                }
            }
            Command::PlayTrack {
                open_url,
                title,
                album,
                reply,
            } => {
                if !reply.is_closed() {
                    let _ = reply.send(guarded(|| driver.play_track(&open_url, &title, &album)));
                }
            }
        }
    }
    drop(driver);
    // SAFETY: balances the successful CoInitializeEx above.
    unsafe { CoUninitialize() };
}
