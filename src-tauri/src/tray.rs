//! The colony's front door.
//!
//! Every cat window is `skipTaskbar`, borderless and always-on-top — which is
//! what makes a cat read as living on the taskbar rather than as an app you
//! switched to. The cost is that Purrch has no presence anywhere in the shell:
//! no taskbar button, no Alt-Tab entry, nothing to click. A cat hidden behind a
//! maximised window was, until this file existed, a cat you could not get back
//! to without closing something.
//!
//! So there is one tray icon for the whole colony. It does the four things that
//! have no other home:
//!
//! * **bring the cats out** — put every one of them on top of whatever buried
//!   them, and stand any that wandered off-screen back on their taskbar;
//! * **start with Windows** — the switch the chore board depends on. "Your PC
//!   is already on, so something may as well be using it" is only true if
//!   Purrch is actually running, and nobody launches a pet by hand every
//!   morning;
//! * **check for updates** — see [`crate::updates`];
//! * **open the log folder** — see [`crate::diag`].
//!
//! Quitting is here too, because the last cat you dismiss quits the app and
//! there needs to be a way to do that without saying goodbye to a cat.

use crate::updates;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

/// Menu item ids. Strings because that is what the event carries.
const SHOW: &str = "show";
const ANOTHER: &str = "another";
const AUTOSTART: &str = "autostart";
const UPDATE: &str = "update";
const LOGS: &str = "logs";
const QUIT: &str = "quit";

/// Whether Purrch is set to launch at login. Never panics on a broken
/// registry — an unreadable setting reads as off, which is the safe default for
/// a checkbox describing something that happens without the user present.
fn starts_with_windows(app: &AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// Puts every cat back in front, and back on screen.
///
/// `show` alone isn't enough: a cat is always-on-top, so a window that has been
/// covered is usually one that lost its topmost status to a full-screen app
/// rather than one that was hidden. Re-asserting both is what actually gets it
/// back, and re-standing it fixes the other way to lose a cat — a monitor that
/// was unplugged while it was standing on it.
pub fn gather(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        let window = window.as_ref().window();
        let _ = window.show();
        let _ = window.set_always_on_top(true);
        crate::restand(&window);
        log::info!("gathered {label}");
    }
}

/// Opens a folder in the platform's file manager.
///
/// Deliberately not a plugin: this is one command, it is only ever handed a
/// path this app built, and pulling in an opener crate to spawn `explorer` is
/// not a trade worth making.
fn reveal(path: &std::path::Path) {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut cmd = std::process::Command::new(program);
    cmd.arg(path);
    crate::bridge::detect::hide_console(&mut cmd);
    // `explorer` returns a non-zero exit code even when it worked, so the
    // result is genuinely not worth looking at.
    let _ = cmd.spawn();
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, SHOW, "here, cat", true, None::<&str>)?;
    let another = MenuItem::with_id(app, ANOTHER, "another cat", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        AUTOSTART,
        "start with Windows",
        true,
        starts_with_windows(app),
        None::<&str>,
    )?;
    let update = MenuItem::with_id(app, UPDATE, "check for updates", true, None::<&str>)?;
    let logs = MenuItem::with_id(app, LOGS, "open log folder", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "quit purrch", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &show,
            &another,
            &PredefinedMenuItem::separator(app)?,
            &autostart,
            &update,
            &logs,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut tray = TrayIconBuilder::with_id("purrch")
        .tooltip("Purrch")
        .menu(&menu)
        // The left click belongs to "show me the cats"; the menu is the right
        // click, the way every other tray icon on the system behaves.
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| {
            let app = app.clone();
            match event.id().as_ref() {
                SHOW => gather(&app),
                ANOTHER => {
                    // From the tray there may be no window to spawn beside, so
                    // this goes through the same path a cat's own menu uses and
                    // simply tolerates having no parent.
                    if let Err(e) = crate::spawn_beside(&app, None) {
                        log::error!("couldn't add a cat from the tray: {e}");
                    }
                }
                AUTOSTART => {
                    let manager = app.autolaunch();
                    let on = manager.is_enabled().unwrap_or(false);
                    let changed = if on {
                        manager.disable()
                    } else {
                        manager.enable()
                    };
                    match changed {
                        Ok(()) => log::info!("start with Windows: {}", !on),
                        Err(e) => log::error!("couldn't change autostart: {e}"),
                    }
                }
                UPDATE => updates::check_from_tray(&app),
                LOGS => match app.path().app_log_dir() {
                    Ok(dir) => {
                        // The folder may not exist yet if nothing has been
                        // logged, and revealing nothing looks like a dead menu.
                        let _ = std::fs::create_dir_all(&dir);
                        reveal(&dir);
                    }
                    Err(e) => log::error!("no log directory: {e}"),
                },
                QUIT => {
                    log::info!("quitting from the tray");
                    app.exit(0);
                }
                other => log::warn!("unknown tray item: {other}"),
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                gather(tray.app_handle());
            }
        });

    // The window icon doubles as the tray icon — it's the same ginger cat, and
    // a second asset would only be a second thing to forget to update.
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
