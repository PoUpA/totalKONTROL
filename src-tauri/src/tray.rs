//! Icon in the system tray: the app keeps running (MIDI, OSC, LEDs) when its
//! window is closed, and quits from the tray menu.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, Window, WindowEvent};

use crate::midi::PadConnection;

const SHOW: &str = "show";
const PANIC: &str = "panic";
const QUIT: &str = "quit";

/// Tray menu items, kept to relabel them when the language changes.
pub struct Tray<R: Runtime> {
    show: MenuItem<R>,
    panic: MenuItem<R>,
    quit: MenuItem<R>,
}

/// Whether the tray icon exists. Without one (no tray on some Linux
/// desktops), closing the window quits as usual.
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Creates the tray icon. Failure is not fatal: the app then works without it.
pub fn setup<R: Runtime>(app: &AppHandle<R>) {
    match build(app) {
        Ok(tray) => {
            app.manage(tray);
            ACTIVE.store(true, Ordering::Relaxed);
        }
        Err(e) => eprintln!("tray icon unavailable: {e}"),
    }
}

fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Tray<R>> {
    let show = MenuItem::with_id(app, SHOW, t!("core.tray.show"), true, None::<&str>)?;
    let panic = MenuItem::with_id(app, PANIC, t!("core.tray.panic"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, t!("core.tray.quit"), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &panic, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("totalKONTROL")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            SHOW => show_window(app),
            // Nothing to stop when not connected.
            PANIC => {
                let _ = app.state::<PadConnection>().panic();
            }
            QUIT => request_quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(Tray { show, panic, quit })
}

/// Menu texts in the current language.
pub fn relabel<R: Runtime>(app: &AppHandle<R>) {
    if let Some(tray) = app.try_state::<Tray<R>>() {
        let _ = tray.show.set_text(t!("core.tray.show"));
        let _ = tray.panic.set_text(t!("core.tray.panic"));
        let _ = tray.quit.set_text(t!("core.tray.quit"));
    }
}

/// Quit asked from the tray, not answered yet by the window.
static QUIT_ASKED: AtomicBool = AtomicBool::new(false);

/// Asks the window first, which may hold unsaved profile changes: it quits
/// (`quit`) or not (`cancel_quit`). Asked again before it answered, as when
/// the window does not respond, the app quits.
fn request_quit<R: Runtime>(app: &AppHandle<R>) {
    if QUIT_ASKED.swap(true, Ordering::SeqCst) {
        app.exit(0);
        return;
    }
    show_window(app);
    if app.emit("quit-requested", ()).is_err() {
        app.exit(0);
    }
}

/// The user kept the app open after all.
pub fn cancel_quit() {
    QUIT_ASKED.store(false, Ordering::SeqCst);
}

fn show_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Closing the window hides it in the tray instead of quitting.
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event
        && ACTIVE.load(Ordering::Relaxed)
    {
        api.prevent_close();
        let _ = window.hide();
    }
}
