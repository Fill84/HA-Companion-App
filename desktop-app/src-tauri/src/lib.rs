use std::sync::Arc;
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Manager, RunEvent, WindowEvent,
    Emitter,
};
use tokio::sync::Mutex;

mod commands;
mod ha_client;
mod registration;
mod sensors;
mod settings;

use commands::*;
use ha_client::HaClient;
use sensors::collector::SensorCollector;
use settings::AppSettings;

/// Shared application state
pub struct AppState {
    pub settings: Mutex<AppSettings>,
    pub ha_client: Mutex<HaClient>,
    pub collector: Mutex<SensorCollector>,
    pub is_registered: Mutex<bool>,
}

pub fn run(dev_mode: bool) {
    // In dev/debug builds, init logger so log::info!/error! show in terminal
    if dev_mode || cfg!(debug_assertions) {
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .try_init();
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus main window when second instance is launched
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(move |app| {
            let handle = app.handle().clone();

            // Load settings
            let app_settings = AppSettings::load(&handle);
            let ha_client = HaClient::new(
                app_settings.server_url.clone(),
                app_settings.access_token.clone(),
                app_settings.webhook_id.clone(),
            );
            let collector = SensorCollector::new(&app_settings.enabled_sensors);

            // Create shared state
            let state = Arc::new(AppState {
                settings: Mutex::new(app_settings.clone()),
                ha_client: Mutex::new(ha_client),
                collector: Mutex::new(collector),
                is_registered: Mutex::new(app_settings.webhook_id.is_some()),
            });

            app.manage(state.clone());

            // Build tray menu
            let show_hide = MenuItemBuilder::with_id("show_hide", "Show / Hide")
                .build(app)?;
            let settings_item = MenuItemBuilder::with_id("settings", "Settings")
                .build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit")
                .build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&show_hide)
                .item(&settings_item)
                .separator()
                .item(&quit)
                .build()?;

            // Build tray icon (from_bytes decodes .ico; path is relative to this source file)
            let icon_bytes = include_bytes!("../icons/icon.ico");
            let icon = Image::from_bytes(icon_bytes).expect("tray icon: invalid icon.ico");
            let _tray = TrayIconBuilder::new()
                .icon(icon)
                .tooltip("Home Assistant Companion")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show_hide" => {
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                    "settings" => {
                        // Close the HA overlay so the main HTML is visible
                        crate::commands::close_dashboard_view(app);
                        // Show window + emit event so JS opens the settings modal
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = window.emit("tray-show-settings", ());
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            // When --dev is used, devtools can be enabled (e.g. via cargo tauri dev).
            // In production, F12 is disabled by the deny-internal-toggle-devtools capability.
            let _ = dev_mode;

            // Spawn background sensor update loop
            let bg_state = state.clone();
            let bg_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                sensor_update_loop(bg_state, bg_handle).await;
            });

            // Show the main window — the JS initApp() will decide what to show.
            // If already registered it will call load_dashboard to add the HA child webview.
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            register_device,
            get_sensor_list,
            update_sensors_now,
            toggle_sensor,
            get_current_language,
            get_my_public_ip,
            load_dashboard,
            hide_dashboard,
        ])
        .build(tauri::generate_context!())
        .expect("Error building Tauri application");

    app.run(|app_handle, event| {
        match event {
            RunEvent::WindowEvent {
                label,
                event: WindowEvent::CloseRequested { api, .. },
                ..
            } => {
                // Hide main window instead of closing (keep in tray)
                if label == "main" {
                    api.prevent_close();
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.hide();
                    }
                }
            }
            _ => {}
        }
    });
}

/// Background task that periodically updates sensors
async fn sensor_update_loop(state: Arc<AppState>, _handle: tauri::AppHandle) {
    // Wait a bit for app to initialize
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    loop {
        let interval_secs = {
            let settings = state.settings.lock().await;
            settings.update_interval
        };

        let is_registered = *state.is_registered.lock().await;

        if is_registered {
            // Collect sensor data
            let sensor_data = {
                let mut collector = state.collector.lock().await;
                collector.collect_dynamic()
            };

            // Send to HA
            let ha_client = state.ha_client.lock().await;
            if let Err(e) = ha_client.update_sensors(&sensor_data).await {
                log::error!("Failed to update sensors: {}", e);

                // If 410 Gone, we need to re-register
                if e.to_string().contains("410") {
                    log::warn!("Webhook expired, need to re-register");
                    *state.is_registered.lock().await = false;
                }
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;
    }
}
