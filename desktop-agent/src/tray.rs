//! Tray icon: shows the pairing code, lists paired devices with "Revoke", and Quit.
//!
//! The tao event loop must run on the main thread. The menu is rebuilt whenever the pairing
//! code or the device list changes (polled twice a second).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::platform::run_return::EventLoopExtRunReturn;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use am_remote_agent::api::AppState;

const POLL: Duration = Duration::from_millis(500);
const ICON_SIZE: u32 = 32;

/// What the menu currently shows; the menu is rebuilt only when this changes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    code: String,
    /// `(name, token)` of each paired device.
    devices: Vec<(String, String)>,
}

fn snapshot(state: &AppState) -> Snapshot {
    Snapshot {
        code: state.pairing_code(),
        devices: state
            .config
            .devices()
            .into_iter()
            .map(|d| (d.name, d.token))
            .collect(),
    }
}

struct BuiltMenu {
    menu: Menu,
    quit: MenuId,
    /// Menu item id -> token of the device it revokes.
    revoke: HashMap<MenuId, String>,
}

fn build_menu(snap: &Snapshot) -> Result<BuiltMenu> {
    let menu = Menu::new();
    let code = MenuItem::new(format!("Pairing code: {}", snap.code), false, None);
    menu.append(&code)?;
    menu.append(&PredefinedMenuItem::separator())?;

    let mut revoke = HashMap::new();
    if snap.devices.is_empty() {
        menu.append(&MenuItem::new("No paired devices", false, None))?;
    } else {
        let devices = Submenu::new(format!("Paired devices ({})", snap.devices.len()), true);
        for (index, (name, token)) in snap.devices.iter().enumerate() {
            let same_name = snap.devices.iter().filter(|(n, _)| n == name).count();
            let label = if same_name > 1 {
                // Identical names would be indistinguishable; number them in pairing order.
                format!("Revoke {name} (#{})", index + 1)
            } else {
                format!("Revoke {name}")
            };
            let item = MenuItem::new(label, true, None);
            revoke.insert(item.id().clone(), token.clone());
            devices.append(&item)?;
        }
        menu.append(&devices)?;
    }

    menu.append(&PredefinedMenuItem::separator())?;
    let quit_item = MenuItem::new("Quit", true, None);
    let quit = quit_item.id().clone();
    menu.append(&quit_item)?;
    Ok(BuiltMenu { menu, quit, revoke })
}

/// A plain filled circle, drawn in code so the agent needs no asset files.
fn icon() -> Result<Icon> {
    let size = ICON_SIZE as f32;
    let (centre, radius) = (size / 2.0, size / 2.0 - 1.5);
    let mut rgba = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let (dx, dy) = (x as f32 + 0.5 - centre, y as f32 + 0.5 - centre);
            if (dx * dx + dy * dy).sqrt() <= radius {
                rgba.extend_from_slice(&[0xFA, 0x24, 0x3C, 0xFF]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    Icon::from_rgba(rgba, ICON_SIZE, ICON_SIZE).context("cannot build the tray icon")
}

/// Runs the tray until the user picks Quit. Blocks the calling (main) thread.
pub fn run(state: &AppState) -> Result<()> {
    let mut event_loop = EventLoopBuilder::new().build();
    let mut tray: Option<TrayIcon> = None;
    let mut shown: Option<Snapshot> = None;
    let mut built: Option<BuiltMenu> = None;
    let mut failure: Option<anyhow::Error> = None;

    event_loop.run_return(|event, _, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + POLL);

        if let Event::NewEvents(StartCause::Init) = event {
            match icon().and_then(|icon| {
                TrayIconBuilder::new()
                    .with_tooltip("am-remote agent")
                    .with_icon(icon)
                    .build()
                    .map_err(|e| anyhow!("cannot create the tray icon: {e}"))
            }) {
                Ok(t) => tray = Some(t),
                Err(e) => {
                    failure = Some(e);
                    *flow = ControlFlow::Exit;
                    return;
                }
            }
        }

        while let Ok(menu_event) = MenuEvent::receiver().try_recv() {
            let Some(menu) = built.as_ref() else { continue };
            if menu_event.id == menu.quit {
                *flow = ControlFlow::Exit;
            } else if let Some(token) = menu.revoke.get(&menu_event.id) {
                match state.config.revoke(token) {
                    Ok(_) => tracing::info!("device revoked from the tray"),
                    Err(e) => tracing::error!("revoke failed to save: {e:#}"),
                }
            }
        }

        let Some(tray) = tray.as_ref() else { return };
        let now = snapshot(state);
        if shown.as_ref() != Some(&now) {
            match build_menu(&now) {
                Ok(menu) => {
                    tray.set_menu(Some(Box::new(menu.menu.clone())));
                    built = Some(menu);
                    shown = Some(now);
                }
                Err(e) => {
                    failure = Some(e);
                    *flow = ControlFlow::Exit;
                }
            }
        }
    });

    failure.map_or(Ok(()), Err)
}
