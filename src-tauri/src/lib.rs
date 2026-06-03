mod clipboard_watch;
mod commands;
mod db;

use clipboard_watch::LastHash;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

/// App-wide shared state: the SQLite connection and the last-seen clipboard hash.
pub struct AppState {
    pub db: Mutex<Connection>,
    pub last_hash: LastHash,
}

fn toggle_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Open Stash", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    // Monochrome backpack silhouette; `icon_as_template` lets macOS tint it for the menu bar.
    TrayIconBuilder::new()
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(false) // left-click toggles the window instead
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => toggle_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Open (or create) the SQLite database in the app data dir.
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let conn = Connection::open(data_dir.join("stash.db"))?;
            db::init(&conn)?;

            let last_hash: LastHash = Arc::new(Mutex::new(None));
            app.manage(AppState {
                db: Mutex::new(conn),
                last_hash: last_hash.clone(),
            });

            // Start the background clipboard watcher.
            clipboard_watch::spawn(app.handle().clone(), last_hash);

            // Menu-bar tray icon.
            setup_tray(app)?;

            // Menu-bar-only app: no dock icon on macOS.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Present the window on first launch (switching activation policy can leave
            // an accessory app hidden). The tray icon toggles it from here on.
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::add_item,
            commands::list_items,
            commands::update_item,
            commands::delete_item,
            commands::list_folders,
            commands::add_folder,
            commands::delete_folder,
            commands::copy_to_clipboard,
            commands::open_url,
            commands::download_item,
            commands::fetch_link_preview,
            commands::export_data,
        ])
        .run(tauri::generate_context!())
        .expect("error while running stash");
}
