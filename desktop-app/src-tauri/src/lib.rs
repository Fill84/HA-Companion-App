use std::sync::Arc;
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItem, MenuItemBuilder},
    tray::TrayIconBuilder,
    Emitter, Manager, RunEvent, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt as _;
use tokio::sync::Mutex;

mod commands;
mod ha_client;
mod logging;
mod registration;
mod sensors;
mod settings;
mod shutdown_hook;

use commands::{mark_unregistered, *};
use ha_client::{HaClient, RegistrationRequest};
use sensors::collector::{SensorCollector, SensorValue};
use settings::AppSettings;

/// Shared application state
pub struct AppState {
    pub app_handle: tauri::AppHandle,
    pub settings_load_error: Option<String>,
    pub settings: Mutex<AppSettings>,
    pub ha_client: Mutex<HaClient>,
    pub collector: Mutex<SensorCollector>,
    pub is_registered: Mutex<bool>,
    pub registration_lock: Mutex<()>,
}

struct TrayItems {
    show_hide: MenuItem<tauri::Wry>,
    settings: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

fn tray_labels(language: &str) -> (&'static str, &'static str, &'static str) {
    if language == "nl" {
        ("Tonen / Verbergen", "Instellingen", "Afsluiten")
    } else {
        ("Show / Hide", "Settings", "Quit")
    }
}

fn update_tray_labels(app: &tauri::AppHandle, language: &str) {
    if let Some(items) = app.try_state::<TrayItems>() {
        let (show_hide, settings, quit) = tray_labels(language);
        for result in [
            items.show_hide.set_text(show_hide),
            items.settings.set_text(settings),
            items.quit.set_text(quit),
        ] {
            if let Err(error) = result {
                log::warn!("Could not update tray label: {error}");
            }
        }
    }
}

/// Run potentially slow hardware reads on Tokio's blocking pool.
pub async fn collect_snapshot(
    state: Arc<AppState>,
    full: bool,
) -> Result<Vec<SensorValue>, String> {
    let worker_state = state.clone();
    let (sensors, identities) = tokio::task::spawn_blocking(move || {
        let mut collector = worker_state.collector.blocking_lock();
        let sensors = if full {
            collector.collect_all()
        } else {
            collector.collect_dynamic()
        };
        (sensors, collector.identity_map())
    })
    .await
    .map_err(|error| format!("Sensor collection worker failed: {error}"))?;
    let mut settings = state.settings.lock().await;
    if settings.sensor_identity_map != identities {
        settings.save_identity_map(&state.app_handle, identities)?;
    }
    Ok(sensors)
}

pub fn run(dev_mode: bool) {
    // Always log to file. In dev/debug, also mirror to stderr so the
    // terminal shows live output.
    let file_log_path = logging::log_file_path();
    if let Ok(ref path) = file_log_path {
        if let Err(e) = logging::init_logger(path) {
            // Logger init failed — fall back to env_logger so we at least get
            // stderr output and can diagnose why the file logger failed.
            eprintln!(
                "[bootstrap] file logger init failed: {} — falling back to stderr",
                e
            );
            let _ =
                env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
                    .try_init();
        }
    } else {
        // APPDATA not set (CI/Linux dev) — stderr is fine.
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .try_init();
    }

    log::info!(
        "[bootstrap] HA Companion v{} starting (dev_mode={}, log_file={:?})",
        env!("CARGO_PKG_VERSION"),
        dev_mode,
        file_log_path.as_ref().ok(),
    );

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus main window when second instance is launched
            if let Some(window) = app.get_window("main") {
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
            let (app_settings, settings_load_error) = match AppSettings::load(&handle) {
                Ok(settings) => (settings, None),
                Err(error) => {
                    log::error!("Settings could not be loaded: {error}");
                    (AppSettings::default(), Some(error))
                }
            };
            if settings_load_error.is_none() {
                let autostart_result = handle.autolaunch().is_enabled().and_then(|enabled| {
                    if enabled == app_settings.autostart {
                        Ok(())
                    } else if app_settings.autostart {
                        handle.autolaunch().enable()
                    } else {
                        handle.autolaunch().disable()
                    }
                });
                if let Err(error) = autostart_result {
                    log::warn!("Could not reconcile system autostart: {error}");
                }
            }
            let mut ha_client = HaClient::new(
                app_settings.server_url.clone(),
                app_settings.access_token.clone(),
                app_settings.webhook_id.clone(),
            );
            ha_client.set_update_interval(app_settings.update_interval);
            let collector = SensorCollector::new(
                &app_settings.enabled_sensors,
                &app_settings.sensor_identity_map,
                &app_settings.legacy_gpu_aliases,
            );

            // Create shared state
            let state = Arc::new(AppState {
                app_handle: handle.clone(),
                settings_load_error,
                settings: Mutex::new(app_settings.clone()),
                ha_client: Mutex::new(ha_client),
                collector: Mutex::new(collector),
                is_registered: Mutex::new(app_settings.webhook_id.is_some()),
                registration_lock: Mutex::new(()),
            });

            app.manage(state.clone());

            // Phase 3: Windows shutdown hook — fire a synchronous send of
            // device_offline before Windows reaps the process.
            {
                let hook_state = state.clone();
                let main_window = app.get_window("main").expect("main window");
                shutdown_hook::install(&main_window, move || {
                    let state = hook_state.clone();
                    // Spawn into a Tokio runtime, block briefly so the OS
                    // shutdown handshake waits for the HTTP POST.
                    let _ = std::thread::spawn(move || {
                        if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                        {
                            rt.block_on(async move {
                                let _ = tokio::time::timeout(
                                    std::time::Duration::from_secs(2),
                                    async {
                                        let ha = state.ha_client.lock().await;
                                        ha.send_device_offline().await
                                    },
                                )
                                .await;
                            });
                        }
                    })
                    .join();
                });
            }

            // Build tray menu
            let (show_label, settings_label, quit_label) = tray_labels(&app_settings.language);
            let show_hide = MenuItemBuilder::with_id("show_hide", show_label).build(app)?;
            let settings_item = MenuItemBuilder::with_id("settings", settings_label).build(app)?;
            let quit = MenuItemBuilder::with_id("quit", quit_label).build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&show_hide)
                .item(&settings_item)
                .separator()
                .item(&quit)
                .build()?;
            app.manage(TrayItems {
                show_hide: show_hide.clone(),
                settings: settings_item.clone(),
                quit: quit.clone(),
            });

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
                        if let Some(window) = app.get_window("main") {
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
                        if let Some(window) = app.get_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                        // Emit on the app handle (works for all webviews)
                        let _ = app.emit("tray-show-settings", ());
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_window("main") {
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

            // Spawn background sensor update loop
            let bg_state = state.clone();
            let bg_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                sensor_update_loop(bg_state, bg_handle).await;
            });

            // Show the main window — the JS initApp() will decide what to show.
            // If already registered it will call load_dashboard to add the HA child webview.
            if let Some(w) = app.get_window("main") {
                let _ = w.show();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            get_integration_version,
            save_settings,
            register_device,
            reregister_device,
            check_connection,
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
                    // Use get_window (not get_webview_window) because with the
                    // unstable multi-webview feature, Window and Webview are separate.
                    if let Some(window) = app_handle.get_window("main") {
                        let _ = window.hide();
                    }
                }
            }
            RunEvent::ExitRequested { .. } => {
                // Phase 3: User chose "Quit" from the tray. Best-effort send
                // device_offline (2s timeout) so HA flips offline immediately.
                let state: Arc<AppState> = app_handle.state::<Arc<AppState>>().inner().clone();
                let _ = std::thread::spawn(move || {
                    if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        rt.block_on(async move {
                            let _ =
                                tokio::time::timeout(std::time::Duration::from_secs(2), async {
                                    let ha = state.ha_client.lock().await;
                                    ha.send_device_offline().await
                                })
                                .await;
                        });
                    }
                })
                .join();
            }
            _ => {}
        }
    });
}

/// True if the error indicates the webhook no longer exists on HA's side.
/// 404 = webhook handler is gone (integration removed/restarted, storage wiped).
/// 410 = webhook handler exists in HA but our config entry is missing.
/// In both cases the local webhook_id is dead and we must re-register.
pub(crate) fn is_webhook_dead(err: &str) -> bool {
    matches!(webhook_http_status(err), Some(404 | 410))
}

pub(crate) fn webhook_http_status(err: &str) -> Option<u16> {
    err.split_once("returned HTTP ")
        .and_then(|(_, status)| status.split_whitespace().next())
        .and_then(|status| status.parse().ok())
}

/// Background task that periodically updates sensors
async fn sensor_update_loop(state: Arc<AppState>, handle: tauri::AppHandle) {
    // Wait a bit for app to initialize
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    if *state.is_registered.lock().await {
        refresh_device_metadata(&state).await;
    }

    let mut cycle_count: u64 = 0;
    let mut next_registration_attempt = tokio::time::Instant::now();

    loop {
        let interval_secs = {
            let settings = state.settings.lock().await;
            settings.update_interval
        };

        let is_registered = *state.is_registered.lock().await;

        if is_registered {
            if cycle_count.is_multiple_of(10) {
                let all_sensors = match collect_snapshot(state.clone(), true).await {
                    Ok(sensors) => sensors,
                    Err(error) => {
                        log::error!("{error}");
                        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;
                        continue;
                    }
                };
                let ha_client = state.ha_client.lock().await;
                if let Err(e) = ha_client.register_sensors(&all_sensors).await {
                    log::error!("Failed to re-register sensors: {}", e);
                    let err_str = e.to_string();
                    let failed_webhook = ha_client.webhook_id().unwrap_or_default().to_owned();
                    drop(ha_client);
                    if is_webhook_dead(&err_str) {
                        mark_unregistered(&state, &handle, &failed_webhook, &err_str).await;
                    }
                } else {
                    log::debug!("Re-registered {} sensors with HA", all_sensors.len());
                    if let Err(e) = ha_client.update_sensors(&all_sensors, "all").await {
                        log::error!("Failed to update all sensors: {}", e);
                        let err_str = e.to_string();
                        let failed_webhook = ha_client.webhook_id().unwrap_or_default().to_owned();
                        drop(ha_client);
                        if is_webhook_dead(&err_str) {
                            mark_unregistered(&state, &handle, &failed_webhook, &err_str).await;
                        }
                    }
                }
            } else {
                let sensor_data = match collect_snapshot(state.clone(), false).await {
                    Ok(sensors) => sensors,
                    Err(error) => {
                        log::error!("{error}");
                        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;
                        continue;
                    }
                };

                let ha_client = state.ha_client.lock().await;
                if let Err(e) = ha_client.update_sensors(&sensor_data, "dynamic").await {
                    log::error!("Failed to update sensors: {}", e);
                    let err_str = e.to_string();
                    let failed_webhook = ha_client.webhook_id().unwrap_or_default().to_owned();
                    drop(ha_client);
                    if is_webhook_dead(&err_str) {
                        mark_unregistered(&state, &handle, &failed_webhook, &err_str).await;
                    }
                }
            }

            cycle_count += 1;
        } else if tokio::time::Instant::now() >= next_registration_attempt {
            let configured = {
                let settings = state.settings.lock().await;
                !settings.server_url.is_empty() && !settings.access_token.is_empty()
            };
            if configured {
                if let Ok(_guard) = state.registration_lock.try_lock() {
                    if !*state.is_registered.lock().await {
                        match crate::commands::register_device_inner(&state, &handle).await {
                            Ok(()) => {
                                log::info!("Background registration recovered");
                                if let Err(error) = handle.emit("registration-restored", ()) {
                                    log::warn!("Could not announce restored registration: {error}");
                                }
                                cycle_count = 0;
                            }
                            Err(error) => log::warn!("Background registration failed: {error}"),
                        }
                    }
                }
                next_registration_attempt =
                    tokio::time::Instant::now() + tokio::time::Duration::from_secs(60);
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;
    }
}

async fn refresh_device_metadata(state: &AppState) {
    let device_id = state.settings.lock().await.device_id.clone();
    let info = match tokio::task::spawn_blocking(crate::sensors::system_info::collect).await {
        Ok(info) => info,
        Err(error) => {
            log::warn!("Could not collect device metadata: {error}");
            return;
        }
    };
    let request = RegistrationRequest {
        device_id,
        device_name: info.hostname,
        manufacturer: info.motherboard_manufacturer,
        model: info.motherboard_model,
        os_name: Some(info.os_name),
        os_version: Some(info.os_version),
        app_version: Some(env!("CARGO_PKG_VERSION").into()),
    };
    if let Err(error) = state
        .ha_client
        .lock()
        .await
        .update_registration(&request)
        .await
    {
        log::warn!("Could not refresh device metadata: {error}");
    }
}

#[cfg(test)]
mod webhook_status_tests {
    use super::is_webhook_dead;

    #[test]
    fn only_explicit_missing_webhook_responses_clear_registration() {
        assert!(is_webhook_dead(
            "register_sensor returned HTTP 404 Not Found"
        ));
        assert!(is_webhook_dead(
            "update_sensor_states returned HTTP 410 Gone"
        ));
        assert!(!is_webhook_dead("Webhook connection failed on port 4040"));
        assert!(!is_webhook_dead("register_sensor returned HTTP 4040"));
        assert!(!is_webhook_dead(
            "update_sensor_states returned HTTP 500 Internal Server Error"
        ));
    }
}
