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
use windows::core::{Interface, BOOL, BSTR, HSTRING};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern,
    IUIAutomationScrollItemPattern, IUIAutomationScrollPattern, IUIAutomationSelectionItemPattern,
    IUIAutomationTogglePattern, IUIAutomationValuePattern, ScrollAmount_LargeIncrement,
    ScrollAmount_NoAmount, TreeScope_Descendants, UIA_AutomationIdPropertyId,
    UIA_ButtonControlTypeId, UIA_ControlTypePropertyId, UIA_EditControlTypeId, UIA_InvokePatternId,
    UIA_IsScrollPatternAvailablePropertyId, UIA_ListItemControlTypeId, UIA_MenuItemControlTypeId,
    UIA_NamePropertyId, UIA_ScrollItemPatternId, UIA_ScrollPatternId, UIA_SelectionItemPatternId,
    UIA_TextControlTypeId, UIA_TogglePatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowThreadProcessId, ShowWindow, SW_RESTORE, SW_SHOWNORMAL,
};

use super::appsearch::{
    can_play, parse_result, AppSearchResult, Kind, Scope, SCOPE_CATALOG_ID, SCOPE_LIBRARY_ID,
    SEARCH_BOX_ID, SEARCH_BUTTON_ID, STATUS_TEXT_ID, SUGGESTIONS_LIST_ID,
};
use super::audio::{is_apple_music_path, process_path};
use super::library::{
    find_track_row, is_page_row_name, menu_item_plays, playlist_from_sidebar_item, same_text,
    Playlist, PAGE_HEADER_ID, PLAYLISTS_HEADER_ID, PLAY_BUTTON_ID,
};
use super::modes::{repeat_from_name, repeat_presses, Modes, Repeat};
use super::modes::{REPEAT_BUTTON_ID, SHUFFLE_BUTTON_ID};
use super::queue::{queue_item, QueueItem, QueueSource};
use super::queue::{AUTOPLAY_LIST_ID, MAX_ROWS, QUEUE_LIST_ID, QUEUE_TOGGLE_ID};
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
    /// The UI was driven as intended but did not end up in the asked state, or did not answer in
    /// time (shuffle or repeat did not change, a search did not finish): contract `500`.
    Unchanged(String),
}

impl From<UiaError> for MediaError {
    fn from(error: UiaError) -> Self {
        match error {
            UiaError::NotRunning => Self::NotRunning,
            UiaError::NotFound(message) => Self::NotFound(message),
            UiaError::Failed(message) => Self::NotSupported(message),
            UiaError::Unchanged(message) => Self::Internal(message),
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
        self.play_song_row(&window, title)
    }

    /// On an open album page: finds the song's row (scrolling down if needed), opens its menu and
    /// presses `Play “<title>”`.
    fn play_song_row(&self, window: &IUIAutomationElement, title: &str) -> Result<(), UiaError> {
        // The song row: scroll the page down until it shows up.
        let mut scrolls = 0;
        let row = wait_for(PAGE_WAIT, || {
            let rows = self.page_rows(window);
            let names: Vec<String> = rows.iter().map(|(_, name)| name.clone()).collect();
            if let Some(index) = find_track_row(&names, title) {
                return rows.into_iter().nth(index).map(|(row, _)| row);
            }
            if !rows.is_empty() && scrolls < MAX_SCROLLS && self.scroll_page_down(window) {
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

// ---------- MVP 4: shuffle and repeat, the queue, search through the app ----------

/// AutomationId of the transport bar that holds shuffle, repeat and the play controls.
// verified: spike mvp4, 2026-11-14
const TRANSPORT_BAR_ID: &str = "TransportBar";
/// AutomationId shared by the left navigation pane and the right queue pane (two elements).
// verified: spike mvp4, 2026-11-14
const PANE_ID: &str = "PaneRoot";
/// The queue pane's "Playing Next" header (English UI); used only to recognise the pane.
// verified: spike mvp4, 2026-11-14
const PLAYING_NEXT_NAME: &str = "Playing Next";
/// AutomationId of a result section's heading text (the section's Name equals it).
// verified: spike mvp4, 2026-11-14
const SECTION_HEADING_ID: &str = "JustTextBlock";
/// AutomationId of the container a result section wraps its content in (its Name is the heading).
// verified: spike mvp4, 2026-11-14
const SECTION_CONTENT_ID: &str = "ShelfSectionContent";
/// The first section of a catalog result page (it has no heading element of its own).
// verified: spike mvp4, 2026-11-14
const TOP_RESULTS: &str = "Top Results";

/// Shuffle or repeat must show the asked state within this time after the last press.
const MODES_WAIT: Duration = Duration::from_millis(1500);
/// The queue pane must show up within this time after pressing its toolbar button.
const QUEUE_WAIT: Duration = Duration::from_secs(3);
/// The search box must show up within this time after pressing the sidebar search button.
const SEARCH_BOX_WAIT: Duration = Duration::from_secs(4);
/// The box needs a moment before it reacts to text (a `SetValue` 0.2 s after it appeared did
/// nothing, see the spike); and the text is set again if nothing happened within this time.
const BOX_SETTLE: Duration = Duration::from_millis(900);
const SUGGESTION_WAIT: Duration = Duration::from_secs(2);
/// The whole search (open, type, results page) must finish within this time.
const SEARCH_WAIT: Duration = Duration::from_secs(12);
/// A results page that shows its status text but no section for this long has no results.
const EMPTY_RESULTS_WAIT: Duration = Duration::from_millis(2500);
/// Time for the page to re-draw after scrolling or switching the scope.
const SETTLE: Duration = Duration::from_millis(700);
/// Pause after scrolling a page down before the sections are read (1.5 s worked in the spike).
const SECTION_SCROLL_WAIT: Duration = Duration::from_millis(1500);
/// How long a scroll step waits for the visible sections to fill in their items.
const SECTION_FILL_WAIT: Duration = Duration::from_secs(2);
/// How many scroll steps are taken through a result page at most.
const MAX_SECTION_STEPS: usize = 14;
/// Items read per section at most.
const MAX_ITEMS_PER_SECTION: usize = 30;

impl Driver {
    fn is_visible(&self, element: &IUIAutomationElement) -> bool {
        // SAFETY: plain UI Automation call.
        unsafe { element.CurrentIsOffscreen() }
            .map(|b| !b.as_bool())
            .unwrap_or(false)
    }

    fn left_of(&self, element: &IUIAutomationElement) -> i32 {
        // SAFETY: plain UI Automation call.
        unsafe { element.CurrentBoundingRectangle() }
            .map(|r| r.left)
            .unwrap_or(0)
    }

    fn list_items_under(&self, root: &IUIAutomationElement) -> Vec<IUIAutomationElement> {
        self.find_all(
            root,
            UIA_ControlTypePropertyId,
            &VARIANT::from(UIA_ListItemControlTypeId.0),
        )
        .unwrap_or_default()
    }

    /// The texts of an element's `TextBlock`s, in tree order (a row's title, detail, duration).
    fn text_parts(&self, element: &IUIAutomationElement) -> Vec<String> {
        self.find_all(
            element,
            UIA_ControlTypePropertyId,
            &VARIANT::from(UIA_TextControlTypeId.0),
        )
        .unwrap_or_default()
        .iter()
        .map(|text| self.name_of(text))
        .filter(|text| !text.trim().is_empty())
        .collect()
    }

    fn toggle_state(&self, element: &IUIAutomationElement) -> Option<i32> {
        // SAFETY: plain UI Automation calls.
        unsafe {
            element
                .GetCurrentPattern(UIA_TogglePatternId)
                .and_then(|p| p.cast::<IUIAutomationTogglePattern>())
                .and_then(|p| p.CurrentToggleState())
        }
        .ok()
        .map(|state| state.0)
    }

    fn press_toggle(&self, element: &IUIAutomationElement, what: &str) -> Result<(), UiaError> {
        // SAFETY: plain UI Automation calls.
        unsafe {
            element
                .GetCurrentPattern(UIA_TogglePatternId)
                .and_then(|p| p.cast::<IUIAutomationTogglePattern>())
                .and_then(|p| p.Toggle())
        }
        .map_err(|e| failed(&format!("cannot switch {what}"), e))
    }

    /// The main window only if it is already visible: never restores a minimized one.
    fn visible_main_window(&self) -> Option<IUIAutomationElement> {
        self.apple_music_windows(Some(WINDOW_CLASS))
            .into_iter()
            .find(|w| self.is_visible(w) && self.find_by_id(w, NAV_VIEW_ID).is_some())
    }

    // ---------- shuffle and repeat ----------

    /// Shuffle and repeat as the two transport-bar buttons show them.
    fn read_modes(&self, window: &IUIAutomationElement) -> Result<Modes, UiaError> {
        let bar = self
            .find_by_id(window, TRANSPORT_BAR_ID)
            .unwrap_or_else(|| window.clone());
        let shuffle_button = self
            .find_by_id(&bar, SHUFFLE_BUTTON_ID)
            .ok_or_else(|| UiaError::Failed("the shuffle button was not found".into()))?;
        let repeat_button = self
            .find_by_id(&bar, REPEAT_BUTTON_ID)
            .ok_or_else(|| UiaError::Failed("the repeat button was not found".into()))?;
        let shuffle = match self.toggle_state(&shuffle_button) {
            Some(0) => false,
            Some(1) => true,
            _ => return Err(UiaError::Failed("the shuffle state cannot be read".into())),
        };
        let repeat = repeat_from_name(&self.name_of(&repeat_button))
            .ok_or_else(|| UiaError::Failed("the repeat state cannot be read".into()))?;
        Ok(Modes { shuffle, repeat })
    }

    fn modes(&self) -> Result<Modes, UiaError> {
        let window = self.main_window()?;
        self.read_modes(&window)
    }

    /// For polling: `None` unless the window is visible and both buttons read fine.
    fn modes_if_visible(&self) -> Option<Modes> {
        let window = self.visible_main_window()?;
        self.read_modes(&window).ok()
    }

    fn set_modes(&self, shuffle: Option<bool>, repeat: Option<Repeat>) -> Result<Modes, UiaError> {
        let window = self.main_window()?;
        let current = self.read_modes(&window)?;
        let bar = self
            .find_by_id(&window, TRANSPORT_BAR_ID)
            .unwrap_or_else(|| window.clone());

        if let Some(want) = shuffle.filter(|want| *want != current.shuffle) {
            let button = self
                .find_by_id(&bar, SHUFFLE_BUTTON_ID)
                .ok_or_else(|| UiaError::Failed("the shuffle button was not found".into()))?;
            self.press_toggle(&button, "shuffle")?;
            let _ = want;
        }
        if let Some(want) = repeat {
            for _ in 0..repeat_presses(current.repeat, want) {
                let button = self
                    .find_by_id(&bar, REPEAT_BUTTON_ID)
                    .ok_or_else(|| UiaError::Failed("the repeat button was not found".into()))?;
                let before = self.name_of(&button);
                self.invoke(&button, "the repeat button")?;
                // Wait for the label to change before pressing again.
                let _ = wait_for(MODES_WAIT, || {
                    let now = self.find_by_id(&bar, REPEAT_BUTTON_ID)?;
                    (self.name_of(&now) != before).then_some(())
                });
            }
        }
        wait_for(MODES_WAIT, || {
            self.read_modes(&window).ok().filter(|modes| {
                shuffle.is_none_or(|want| modes.shuffle == want)
                    && repeat.is_none_or(|want| modes.repeat == want)
            })
        })
        .ok_or_else(|| {
            UiaError::Unchanged("Apple Music did not change shuffle or repeat as asked".into())
        })
    }

    // ---------- the queue ----------

    /// Does this pane hold the play queue (the other `PaneRoot` is the navigation sidebar)?
    fn is_queue_pane(&self, pane: &IUIAutomationElement) -> bool {
        self.find_by_id(pane, QUEUE_LIST_ID).is_some()
            || self.find_by_id(pane, AUTOPLAY_LIST_ID).is_some()
            || self
                .find_all(pane, UIA_NamePropertyId, &VARIANT::from(PLAYING_NEXT_NAME))
                .is_ok_and(|found| !found.is_empty())
    }

    fn queue_pane(&self, window: &IUIAutomationElement) -> Option<IUIAutomationElement> {
        self.find_all(window, UIA_AutomationIdPropertyId, &VARIANT::from(PANE_ID))
            .ok()?
            .into_iter()
            .filter(|pane| self.is_queue_pane(pane))
            .max_by_key(|pane| self.left_of(pane))
    }

    /// The queue pane, opened through its toolbar button if it was closed (it is left open).
    /// The flag says whether it had to be opened just now.
    fn open_queue_pane(
        &self,
        window: &IUIAutomationElement,
    ) -> Result<(IUIAutomationElement, bool), UiaError> {
        if let Some(pane) = self.queue_pane(window) {
            return Ok((pane, false));
        }
        let button = self
            .find_by_id(window, QUEUE_TOGGLE_ID)
            .ok_or_else(|| UiaError::Failed("the queue button was not found".into()))?;
        if self.toggle_state(&button) != Some(1) {
            self.press_toggle(&button, "the queue pane")?;
        }
        let pane = wait_for(QUEUE_WAIT, || self.queue_pane(window))
            .ok_or_else(|| UiaError::Failed("the queue pane did not open".into()))?;
        Ok((pane, true))
    }

    /// The rows of "Playing Next" then "AutoPlay", at most [`MAX_ROWS`], numbered across both.
    fn read_queue_rows(&self, pane: &IUIAutomationElement) -> Vec<QueueItem> {
        let mut items: Vec<QueueItem> = Vec::new();
        for (list_id, source) in [
            (QUEUE_LIST_ID, QueueSource::Queue),
            (AUTOPLAY_LIST_ID, QueueSource::Autoplay),
        ] {
            let Some(list) = self.find_by_id(pane, list_id) else {
                continue;
            };
            for row in self.list_items_under(&list) {
                if items.len() >= MAX_ROWS {
                    return items;
                }
                if let Some(item) = queue_item(items.len(), source, &self.text_parts(&row)) {
                    items.push(item);
                }
            }
        }
        items
    }

    fn queue(&self) -> Result<Vec<QueueItem>, UiaError> {
        let window = self.main_window()?;
        let (pane, just_opened) = self.open_queue_pane(&window)?;
        if just_opened {
            // The rows are filled in a moment after the pane appears.
            thread::sleep(SETTLE);
        }
        Ok(self.read_queue_rows(&pane))
    }

    // ---------- search through the app ----------

    fn search_box(&self, window: &IUIAutomationElement) -> Option<IUIAutomationElement> {
        self.find_all(
            window,
            UIA_AutomationIdPropertyId,
            &VARIANT::from(SEARCH_BOX_ID),
        )
        .ok()?
        .into_iter()
        .find(|element| {
            // SAFETY: plain UI Automation call.
            let is_edit = unsafe { element.CurrentControlType() } == Ok(UIA_EditControlTypeId);
            is_edit && self.is_visible(element)
        })
    }

    fn set_search_text(
        &self,
        search_box: &IUIAutomationElement,
        text: &str,
    ) -> Result<(), UiaError> {
        // SAFETY: plain UI Automation calls; nothing is typed, the value is set through the pattern.
        unsafe {
            search_box
                .GetCurrentPattern(UIA_ValuePatternId)
                .and_then(|p| p.cast::<IUIAutomationValuePattern>())
                .and_then(|p| p.SetValue(&BSTR::from(text)))
        }
        .map_err(|e| failed("cannot set the search text", e))
    }

    /// Does the results page say it shows the results for `query`?
    fn status_matches(&self, window: &IUIAutomationElement, query: &str) -> bool {
        self.find_by_id(window, STATUS_TEXT_ID)
            .is_some_and(|status| {
                self.name_of(&status)
                    .to_lowercase()
                    .contains(&query.to_lowercase())
            })
    }

    /// The suggestion whose text is exactly the query (it runs the full search). The app shows it
    /// lower-cased, so the comparison ignores case.
    fn plain_suggestion(
        &self,
        window: &IUIAutomationElement,
        query: &str,
    ) -> Option<IUIAutomationElement> {
        let list = self.find_by_id(window, SUGGESTIONS_LIST_ID)?;
        self.list_items_under(&list)
            .into_iter()
            .find(|item| self.name_of(item).trim().to_lowercase() == query.to_lowercase())
    }

    /// The result sections currently realised on the page: wide list items whose Name is their own
    /// heading text (or the first section, "Top Results").
    fn page_sections(&self, window: &IUIAutomationElement) -> Vec<(String, IUIAutomationElement)> {
        // SAFETY: plain UI Automation call.
        let window_width = unsafe { window.CurrentBoundingRectangle() }
            .map(|r| r.right - r.left)
            .unwrap_or(0);
        self.list_items_under(window)
            .into_iter()
            .filter_map(|item| {
                // SAFETY: plain UI Automation call.
                let rect = unsafe { item.CurrentBoundingRectangle() }.ok()?;
                // The queue pane's rows and the sidebar are far narrower than the page.
                if (rect.right - rect.left) * 10 < window_width * 4 {
                    return None;
                }
                let name = self.name_of(&item);
                // A section holds a container (or heading text) called like itself.
                let named_like_itself = |id: &str| {
                    self.find_by_id(&item, id)
                        .is_some_and(|inner| self.name_of(&inner) == name)
                };
                let is_section = name == TOP_RESULTS
                    || named_like_itself(SECTION_CONTENT_ID)
                    || named_like_itself(SECTION_HEADING_ID);
                (!name.is_empty() && is_section).then_some((name, item))
            })
            .collect()
    }

    /// Opens the search box if needed, sets the query, runs the search and waits until the results
    /// page is up, then switches to `scope`.
    fn run_search(
        &self,
        window: &IUIAutomationElement,
        query: &str,
        scope: Scope,
    ) -> Result<(), UiaError> {
        let search_box = match self.search_box(window) {
            Some(search_box) => search_box,
            None => {
                let button = self
                    .find_by_id(window, SEARCH_BUTTON_ID)
                    .ok_or_else(|| UiaError::Failed("the search button was not found".into()))?;
                self.invoke(&button, "the search button")?;
                let found = wait_for(SEARCH_BOX_WAIT, || self.search_box(window))
                    .ok_or_else(|| UiaError::Failed("the search box did not open".into()))?;
                thread::sleep(BOX_SETTLE);
                found
            }
        };

        let deadline = Instant::now() + SEARCH_WAIT;
        self.set_search_text(&search_box, query)?;
        let mut set_at = Instant::now();
        let mut attempts = 1;
        let mut suggestion_invoked = false;
        // Either the page updates by itself, or a suggestion equal to the query shows up and is
        // invoked; if neither happens the text is set again (the box ignored it).
        while !self.status_matches(window, query) {
            if Instant::now() >= deadline {
                return Err(UiaError::Unchanged(
                    "Apple Music did not run the search in time".into(),
                ));
            }
            if !suggestion_invoked {
                if let Some(suggestion) = self.plain_suggestion(window, query) {
                    self.invoke(&suggestion, "the search suggestion")?;
                    suggestion_invoked = true;
                } else if set_at.elapsed() >= SUGGESTION_WAIT && attempts < 3 {
                    let again = self
                        .search_box(window)
                        .unwrap_or_else(|| search_box.clone());
                    self.set_search_text(&again, query)?;
                    set_at = Instant::now();
                    attempts += 1;
                }
            }
            thread::sleep(POLL);
        }

        // The page has its status text; wait for the first section (or decide there are none).
        let shown_since = Instant::now();
        while self.page_sections(window).is_empty() && shown_since.elapsed() < EMPTY_RESULTS_WAIT {
            if Instant::now() >= deadline {
                return Err(UiaError::Unchanged(
                    "the search results did not show in time".into(),
                ));
            }
            thread::sleep(POLL);
        }
        self.select_scope(window, scope)
    }

    fn select_scope(&self, window: &IUIAutomationElement, scope: Scope) -> Result<(), UiaError> {
        let id = match scope {
            Scope::Catalog => SCOPE_CATALOG_ID,
            Scope::Library => SCOPE_LIBRARY_ID,
        };
        let button = wait_for(Duration::from_secs(2), || self.find_by_id(window, id))
            .ok_or_else(|| UiaError::Failed("the search scope switch was not found".into()))?;
        if self.toggle_state(&button) != Some(1) {
            self.press_toggle(&button, "the search scope")?;
            wait_for(Duration::from_secs(2), || {
                (self.toggle_state(&button) == Some(1)).then_some(())
            })
            .ok_or_else(|| UiaError::Failed("the search scope did not switch".into()))?;
            // The page re-filters its sections.
            thread::sleep(SETTLE);
            let _ = wait_for(Duration::from_secs(2), || {
                (!self.page_sections(window).is_empty()).then_some(())
            });
        }
        Ok(())
    }

    /// The sections in view with their items, once the page has stopped filling them in (after a
    /// scroll a section shows its heading first and its items a moment later; moving on too early
    /// scrolls it out again before it was ever read).
    fn settled_sections(
        &self,
        window: &IUIAutomationElement,
    ) -> Vec<(String, Vec<IUIAutomationElement>)> {
        let deadline = Instant::now() + SECTION_FILL_WAIT;
        let mut last: Vec<(String, usize)> = Vec::new();
        loop {
            let current: Vec<(String, Vec<IUIAutomationElement>)> = self
                .page_sections(window)
                .into_iter()
                .map(|(name, section)| (name, self.list_items_under(&section)))
                .collect();
            let counts: Vec<(String, usize)> = current
                .iter()
                .map(|(name, items)| (name.clone(), items.len()))
                .collect();
            if (!current.is_empty() && counts == last) || Instant::now() >= deadline {
                return current;
            }
            last = counts;
            thread::sleep(POLL);
        }
    }

    /// The page's vertical scroller: the largest vertically scrollable element (the sidebar and the
    /// queue pane are narrower).
    fn page_scroller(&self, window: &IUIAutomationElement) -> Option<IUIAutomationScrollPattern> {
        let scrollers = self
            .find_all(
                window,
                UIA_IsScrollPatternAvailablePropertyId,
                &VARIANT::from(true),
            )
            .ok()?;
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
        page.map(|(_, pattern)| pattern)
    }

    /// Walks the result page from the top, scrolling a little at a time (only the sections near the
    /// viewport exist in the tree), and calls `visit` once per section, in order of appearance,
    /// with the section's items. `visit` returns true to stop.
    fn walk_sections(
        &self,
        window: &IUIAutomationElement,
        mut visit: impl FnMut(&str, Vec<IUIAutomationElement>) -> bool,
    ) {
        let scroller = self.page_scroller(window);
        if let Some(scroller) = &scroller {
            // SAFETY: plain UI Automation call.
            unsafe {
                let _ = scroller.SetScrollPercent(
                    windows::Win32::UI::Accessibility::UIA_ScrollPatternNoScroll,
                    0.0,
                );
            }
            thread::sleep(SETTLE);
        }
        // A section first shows up with only its heading realised and gets its items as it scrolls
        // into view, so it is visited again whenever more items are there than last time.
        let mut seen: Vec<(String, usize)> = Vec::new();
        for _ in 0..MAX_SECTION_STEPS {
            let settled = self.settled_sections(window);
            for (name, items) in settled {
                let known = seen.iter().position(|(seen_name, _)| *seen_name == name);
                let before = known.map_or(0, |at| seen[at].1);
                if items.len() <= before {
                    continue;
                }
                match known {
                    Some(at) => seen[at].1 = items.len(),
                    None => seen.push((name.clone(), items.len())),
                }
                if visit(&name, items) {
                    return;
                }
            }
            // Half a viewport further down. Whatever lies below the viewport is not realised yet, so
            // the page must be given time to fill in before it is read, and the steps must overlap:
            // a whole page at a time (or a short wait) skipped the Albums and Songs sections.
            let Some(scroller) = &scroller else {
                return;
            };
            // SAFETY: plain UI Automation calls.
            unsafe {
                let percent = scroller.CurrentVerticalScrollPercent().unwrap_or(100.0);
                let view = scroller.CurrentVerticalViewSize().unwrap_or(100.0);
                if percent >= 99.5 || view >= 99.5 {
                    return;
                }
                // `view` is the visible share of the content and the scroll percent a share of
                // the scrollable range, so half a viewport is this many percent of the range.
                let step = (0.5 * (view / 100.0) / (1.0 - view / 100.0) * 100.0).max(8.0);
                let _ = scroller.SetScrollPercent(
                    windows::Win32::UI::Accessibility::UIA_ScrollPatternNoScroll,
                    (percent + step).min(100.0),
                );
            }
            thread::sleep(SECTION_SCROLL_WAIT);
        }
    }

    fn app_search(&self, query: &str, scope: Scope) -> Result<Vec<AppSearchResult>, UiaError> {
        let window = self.main_window()?;
        self.run_search(&window, query, scope)?;
        // Sections in order of first appearance; a section visited again replaces its earlier,
        // shorter list.
        let mut sections: Vec<(String, Vec<AppSearchResult>)> = Vec::new();
        self.walk_sections(&window, |section, items| {
            let mut found: Vec<AppSearchResult> = Vec::new();
            for (index, item) in items.iter().enumerate().take(MAX_ITEMS_PER_SECTION) {
                let parts = self.text_parts(item);
                if let Some(result) = parse_result(section, index, &self.name_of(item), &parts) {
                    found.push(result);
                }
            }
            match sections.iter_mut().find(|(name, _)| name == section) {
                Some((_, known)) => *known = found,
                None => sections.push((section.to_owned(), found)),
            }
            false
        });
        Ok(sections.into_iter().flat_map(|(_, found)| found).collect())
    }

    /// Repeats the search, finds `title` at `section` and `index`, and starts it. Returns the kind
    /// that was started. A song or album *page* reached this way is where playback is started.
    fn app_search_play(
        &self,
        query: &str,
        scope: Scope,
        section: &str,
        index: usize,
        title: &str,
    ) -> Result<Kind, UiaError> {
        let window = self.main_window()?;
        self.run_search(&window, query, scope)?;
        let mut target: Option<(AppSearchResult, IUIAutomationElement)> = None;
        self.walk_sections(&window, |name, items| {
            if !same_text(name, section) {
                return false;
            }
            if let Some(item) = items.get(index) {
                let parts = self.text_parts(item);
                if let Some(result) = parse_result(name, index, &self.name_of(item), &parts) {
                    target = Some((result, item.clone()));
                }
            }
            true
        });
        let (result, item) = target
            .ok_or_else(|| UiaError::NotFound("The search no longer finds that result".into()))?;
        if !same_text(&result.title, title) {
            return Err(UiaError::NotFound(
                "The search no longer finds that result".into(),
            ));
        }
        if !can_play(scope, result.kind) {
            return Err(UiaError::Failed(
                "This kind of result cannot be started yet".into(),
            ));
        }
        self.bring_into_view(&item);
        match (scope, result.kind) {
            // A library song row plays as soon as it is invoked.
            (Scope::Library, Kind::Song) => self.invoke(&item, "the search result")?,
            // A catalog album opens its page; the page's Play button starts it.
            (Scope::Catalog, Kind::Album) => {
                self.invoke(&item, "the search result")?;
                self.press_page_play(&window)?;
            }
            // A catalog song opens its album or single page; play the song from there.
            (Scope::Catalog, Kind::Song) => {
                self.invoke(&item, "the search result")?;
                wait_for(PAGE_WAIT, || self.find_by_id(&window, PLAY_BUTTON_ID)).ok_or_else(
                    || UiaError::Failed("the song's page did not open in Apple Music".into()),
                )?;
                let has_rows = wait_for(Duration::from_secs(2), || {
                    (!self.page_rows(&window).is_empty()).then_some(())
                })
                .is_some();
                // An album page lists its songs (play the right row); a single's page has no list
                // to choose from and its Play button plays the single.
                if has_rows && self.page_rows(&window).len() >= 2 {
                    self.play_song_row(&window, &result.title)?;
                } else {
                    self.press_page_play(&window)?;
                }
            }
            _ => {
                return Err(UiaError::Failed(
                    "This kind of result cannot be started yet".into(),
                ))
            }
        }
        Ok(result.kind)
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
    Modes(Reply<Result<Modes, UiaError>>),
    ModesIfVisible(Reply<Option<Modes>>),
    SetModes {
        shuffle: Option<bool>,
        repeat: Option<Repeat>,
        reply: Reply<Result<Modes, UiaError>>,
    },
    Queue(Reply<Result<Vec<QueueItem>, UiaError>>),
    AppSearch {
        query: String,
        scope: Scope,
        reply: Reply<Result<Vec<AppSearchResult>, UiaError>>,
    },
    AppSearchPlay {
        query: String,
        scope: Scope,
        section: String,
        index: usize,
        title: String,
        reply: Reply<Result<Kind, UiaError>>,
    },
    Stop,
}

/// Waits for the UI thread's answer. An Apple Music that does not answer in time is a failed
/// call (`500`, contract C-007), not a missing feature (`501`).
async fn await_answer<T>(
    limit: Duration,
    answer: oneshot::Receiver<Result<T, UiaError>>,
    stopped: impl FnOnce() -> MediaError,
) -> Result<T, MediaError> {
    match tokio::time::timeout(limit, answer).await {
        Ok(answer) => answer.map_err(|_| stopped())?.map_err(MediaError::from),
        Err(_) => Err(MediaError::Internal(
            "Apple Music took too long to respond".into(),
        )),
    }
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
        await_answer(OP_TIMEOUT, answer, stopped).await
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

    pub async fn modes(&self) -> Result<Modes, MediaError> {
        self.ask(Command::Modes).await
    }

    /// Never touches the window and never waits long: `None` if it cannot be read right now. A
    /// request nobody waits for any more is skipped by the UI thread.
    pub async fn modes_if_visible(&self) -> Option<Modes> {
        let (reply, answer) = oneshot::channel();
        self.commands.send(Command::ModesIfVisible(reply)).ok()?;
        answer.await.ok().flatten()
    }

    pub async fn set_modes(
        &self,
        shuffle: Option<bool>,
        repeat: Option<Repeat>,
    ) -> Result<Modes, MediaError> {
        self.ask(|reply| Command::SetModes {
            shuffle,
            repeat,
            reply,
        })
        .await
    }

    pub async fn queue(&self) -> Result<Vec<QueueItem>, MediaError> {
        self.ask(Command::Queue).await
    }

    pub async fn app_search(
        &self,
        query: &str,
        scope: Scope,
    ) -> Result<Vec<AppSearchResult>, MediaError> {
        let query = query.to_owned();
        self.ask(|reply| Command::AppSearch {
            query,
            scope,
            reply,
        })
        .await
    }

    /// Starts the result; returns the kind that was started.
    pub async fn app_search_play(
        &self,
        query: &str,
        scope: Scope,
        section: &str,
        index: usize,
        title: &str,
    ) -> Result<Kind, MediaError> {
        let (query, section, title) = (query.to_owned(), section.to_owned(), title.to_owned());
        self.ask(|reply| Command::AppSearchPlay {
            query,
            scope,
            section,
            index,
            title,
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
            Command::Modes(reply) => {
                if !reply.is_closed() {
                    let _ = reply.send(guarded(|| driver.modes()));
                }
            }
            Command::ModesIfVisible(reply) => {
                if !reply.is_closed() {
                    let _ = reply.send(guarded(|| Ok(driver.modes_if_visible())).ok().flatten());
                }
            }
            Command::SetModes {
                shuffle,
                repeat,
                reply,
            } => {
                if !reply.is_closed() {
                    let _ = reply.send(guarded(|| driver.set_modes(shuffle, repeat)));
                }
            }
            Command::Queue(reply) => {
                if !reply.is_closed() {
                    let _ = reply.send(guarded(|| driver.queue()));
                }
            }
            Command::AppSearch {
                query,
                scope,
                reply,
            } => {
                if !reply.is_closed() {
                    let _ = reply.send(guarded(|| driver.app_search(&query, scope)));
                }
            }
            Command::AppSearchPlay {
                query,
                scope,
                section,
                index,
                title,
                reply,
            } => {
                if !reply.is_closed() {
                    let _ = reply.send(guarded(|| {
                        driver.app_search_play(&query, scope, &section, index, &title)
                    }));
                }
            }
        }
    }
    drop(driver);
    // SAFETY: balances the successful CoInitializeEx above.
    unsafe { CoUninitialize() };
}

#[cfg(test)]
mod timeout_tests {
    use super::*;

    #[tokio::test]
    async fn no_answer_in_time_is_internal_not_unsupported() {
        let (_reply, answer) = oneshot::channel::<Result<(), UiaError>>();
        let result = await_answer(Duration::from_millis(20), answer, || {
            MediaError::Internal("stopped".into())
        })
        .await;
        assert!(matches!(result, Err(MediaError::Internal(m)) if m.contains("too long")));
    }

    #[tokio::test]
    async fn an_answer_in_time_passes_through() {
        let (reply, answer) = oneshot::channel::<Result<u8, UiaError>>();
        let _ = reply.send(Ok(7));
        let result = await_answer(Duration::from_secs(5), answer, || {
            MediaError::Internal("stopped".into())
        })
        .await;
        assert!(matches!(result, Ok(7)));
    }
}
