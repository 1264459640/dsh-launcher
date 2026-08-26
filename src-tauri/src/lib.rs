mod commands;
mod config;
mod plugins;
mod process;
mod runtime;
mod tasks;
mod tray;
mod windows;

use std::collections::HashMap;
use std::sync::Mutex as StdMutex;
use tauri::{Manager, WindowEvent};

pub struct AppState {
    pub config_path: std::path::PathBuf,
    pub data_dir: std::path::PathBuf,
    pub config: StdMutex<config::Config>,
    pub running: tokio::sync::Mutex<HashMap<String, process::RunningInstance>>,
    pub tasks: tokio::sync::Mutex<HashMap<String, tasks::TaskInfo>>,
    /// Instance whose webview window was opened/focused most recently.
    pub last_focused_instance: StdMutex<Option<String>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let config_path = data_dir.join("config.json");
            let cfg = config::load_config(&config_path);
            app.manage(AppState {
                config_path,
                data_dir: data_dir.clone(),
                config: StdMutex::new(cfg),
                running: tokio::sync::Mutex::new(HashMap::new()),
                tasks: tokio::sync::Mutex::new(HashMap::new()),
                last_focused_instance: StdMutex::new(None),
            });

            // System tray with dynamic menu.
            tray::build_tray(app.handle())?;

            // Close-to-tray for the main window.
            if let Some(win) = app.get_webview_window("main") {
                let handle = app.handle().clone();
                let win2 = win.clone();
                win.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        let minimize = handle
                            .state::<AppState>()
                            .config
                            .lock()
                            .unwrap()
                            .settings
                            .minimize_to_tray;
                        if minimize {
                            api.prevent_close();
                            let _ = win2.hide();
                        }
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_homes,
            commands::create_home,
            commands::default_dedicated_home_path,
            commands::remove_home,
            commands::list_versions,
            commands::fetch_available_versions,
            commands::remove_version,
            tasks::start_create_instance_task,
            tasks::list_tasks,
            tasks::remove_task,
            tasks::cancel_task,
            runtime::get_runtime_status,
            commands::list_instances,
            commands::create_instance,
            commands::update_instance,
            commands::delete_instance,
            commands::copy_instance,
            commands::list_profiles,
            commands::create_profile,
            commands::copy_profile,
            commands::rename_profile,
            commands::delete_profile,
            commands::start_instance,
            commands::stop_instance,
            commands::list_instance_status,
            commands::open_instance_window,
            commands::get_settings,
            commands::update_settings,
            commands::fetch_news,
            plugins::fetch_plugin_market,
            plugins::fetch_plugin_versions,
            plugins::list_installed_plugins,
            plugins::set_plugins_enabled,
            plugins::uninstall_plugin,
            plugins::start_install_plugin_task,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // Terminate child processes when the launcher exits so no DSH
            // instance is left orphaned.
            if let tauri::RunEvent::Exit = event {
                let state = app_handle.state::<AppState>();
                process::kill_all(&state);
            }
        });
}
