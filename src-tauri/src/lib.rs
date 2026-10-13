mod api;
mod config;
mod sentry;

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};

static LOCKED: AtomicBool = AtomicBool::new(true);

#[tauri::command]
fn start_drag(window: tauri::WebviewWindow) {
    let _ = window.start_dragging();
}

#[tauri::command]
fn get_cursor_position(window: tauri::WebviewWindow) -> Result<(f64, f64), String> {
    let cursor = window.cursor_position().map_err(|e| e.to_string())?;
    let win_pos = window.outer_position().map_err(|e| e.to_string())?;
    let scale = window.scale_factor().unwrap_or(2.0);
    // cursor_position() returns absolute screen coords in physical pixels on macOS.
    // Convert to logical pixels relative to window origin.
    Ok((
        (cursor.x - win_pos.x as f64) / scale,
        (cursor.y - win_pos.y as f64) / scale,
    ))
}

/// Current Sentry Mode state for the pet's own indicator. The webview reads it
/// through IPC, so it never needs the HTTP API token.
#[tauri::command]
fn sentry_status(app: AppHandle) -> serde_json::Value {
    sentry::status(&app)
}

fn apply_lock_state(app_handle: &AppHandle, locked: bool) {
    LOCKED.store(locked, Ordering::SeqCst);

    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.set_ignore_cursor_events(locked);
        let _ = window.set_resizable(!locked);
    }

    if let Some(tray) = app_handle.tray_by_id("main-tray") {
        let label = if locked { "Unlock" } else { "Lock" };
        let toggle_i = MenuItem::with_id(app_handle, "toggle", label, true, None::<&str>).unwrap();
        let quit_i = MenuItem::with_id(app_handle, "quit", "Quit", true, None::<&str>).unwrap();
        let menu = Menu::with_items(app_handle, &[&toggle_i, &quit_i]).unwrap();
        let _ = tray.set_menu(Some(menu));
    }

    let _ = app_handle.emit("lock-changed", locked);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            // Restore saved window position & size
            if let Some(state) = config::load_window_state() {
                let _ = window.set_position(tauri::PhysicalPosition::new(state.x, state.y));
                let _ = window.set_size(tauri::PhysicalSize::new(state.width, state.height));
            }

            let _ = window.set_ignore_cursor_events(true);

            let toggle_i = MenuItem::with_id(app, "toggle", "Unlock", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle_i, &quit_i])?;

            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app_handle: &AppHandle, event| match event.id.as_ref() {
                    "toggle" => {
                        let currently_locked = LOCKED.load(Ordering::SeqCst);
                        apply_lock_state(app_handle, !currently_locked);
                    }
                    "quit" => {
                        app_handle.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            // Sentry Mode session state, shared with the HTTP API handlers.
            app.manage(sentry::SentryState::default());

            let cfg = config::load_config();
            println!("Sparky config dir: {}", config::sparky_dir().display());

            let app_handle_clone = app.handle().clone();
            std::thread::spawn(move || {
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(api::start_server(app_handle_clone, cfg));
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(
                event,
                tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_)
            ) {
                if let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) {
                    config::save_window_state(&config::WindowState {
                        x: pos.x,
                        y: pos.y,
                        width: size.width,
                        height: size.height,
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            start_drag,
            get_cursor_position,
            sentry_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
