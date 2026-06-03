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

/// Check the release endpoint for a newer version. When `silent`, only speaks up if an
/// update is found (used on launch); otherwise reports "up to date" / errors too (menu item).
#[cfg(desktop)]
async fn run_update_check(app: tauri::AppHandle, silent: bool) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    use tauri_plugin_updater::UpdaterExt;

    let report = |msg: String, kind: MessageDialogKind| {
        app.dialog().message(msg).kind(kind).title("Stash").blocking_show();
    };

    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            if !silent {
                report(format!("Could not check for updates:\n{e}"), MessageDialogKind::Error);
            }
            return;
        }
    };

    match updater.check().await {
        Ok(Some(update)) => {
            let ver = update.version.clone();
            let notes = update.body.clone().unwrap_or_default();
            let msg = if notes.trim().is_empty() {
                format!("Stash {ver} is available. Install and restart now?")
            } else {
                format!("Stash {ver} is available.\n\n{notes}\n\nInstall and restart now?")
            };
            let install = app
                .dialog()
                .message(msg)
                .title("Update available")
                .buttons(MessageDialogButtons::OkCancelCustom("Install".into(), "Later".into()))
                .blocking_show();
            if install {
                match update.download_and_install(|_chunk, _total| {}, || {}).await {
                    Ok(_) => app.restart(),
                    Err(e) => report(format!("Update failed:\n{e}"), MessageDialogKind::Error),
                }
            }
        }
        Ok(None) => {
            if !silent {
                report("You're on the latest version.".into(), MessageDialogKind::Info);
            }
        }
        Err(e) => {
            if !silent {
                report(format!("Could not check for updates:\n{e}"), MessageDialogKind::Error);
            }
        }
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Open Stash", true, None::<&str>)?;
    let update = MenuItem::with_id(app, "update", "Check for Updates…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &update, &quit])?;

    // Monochrome backpack silhouette; `icon_as_template` lets macOS tint it for the menu bar.
    TrayIconBuilder::new()
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(false) // left-click toggles the window instead
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => toggle_main_window(app),
            "update" => {
                #[cfg(desktop)]
                {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move { run_update_check(app, false).await });
                }
            }
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

            // Auto-updater (desktop only): register the plugin and check quietly on launch.
            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move { run_update_check(handle, true).await });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::add_item,
            commands::list_items,
            commands::update_item,
            commands::set_item_folder,
            commands::delete_item,
            commands::list_folders,
            commands::add_folder,
            commands::delete_folder,
            commands::copy_to_clipboard,
            commands::copy_image_to_clipboard,
            commands::export_image_as,
            commands::open_url,
            commands::download_item,
            commands::fetch_link_preview,
            commands::list_todo_lists,
            commands::add_todo_list,
            commands::rename_todo_list,
            commands::delete_todo_list,
            commands::list_todos,
            commands::add_todo,
            commands::set_todo_done,
            commands::delete_todo,
            commands::clear_completed,
            commands::export_data,
        ])
        .run(tauri::generate_context!())
        .expect("error while running stash");
}
