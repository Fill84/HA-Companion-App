use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use crate::ha_client::normalize_server_url;
use reqwest::Client;
use crate::sensors::collector::SensorListItem;
use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsResponse {
    pub server_url: String,
    pub access_token: String,
    pub webhook_id: Option<String>,
    pub device_id: String,
    pub update_interval: u64,
    pub language: String,
    pub enabled_sensors: HashMap<String, bool>,
    pub autostart: bool,
    pub is_registered: bool,
}

/// Get current settings
#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<SettingsResponse, String> {
    let settings = state.settings.lock().await;
    let is_registered = *state.is_registered.lock().await;

    Ok(SettingsResponse {
        server_url: settings.server_url.clone(),
        access_token: settings.access_token.clone(),
        webhook_id: settings.webhook_id.clone(),
        device_id: settings.device_id.clone(),
        update_interval: settings.update_interval,
        language: settings.language.clone(),
        enabled_sensors: settings.enabled_sensors.clone(),
        autostart: settings.autostart,
        is_registered,
    })
}

/// Save settings and reinitialize connection
#[tauri::command]
pub async fn save_settings(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    server_url: String,
    access_token: String,
    update_interval: u64,
    language: String,
    autostart: bool,
) -> Result<(), String> {
    let server_url = normalize_server_url(&server_url);
    let access_token = access_token.trim().to_string();

    let mut settings = state.settings.lock().await;
    let url_changed = settings.server_url != server_url;
    let token_changed = settings.access_token != access_token;

    settings.server_url = server_url.clone();
    settings.access_token = access_token.clone();
    settings.update_interval = update_interval;
    settings.language = language;
    settings.autostart = autostart;

    if let Err(e) = settings.save(&app) {
        log::error!("[HA] Save settings failed: {}", e);
        return Err(e);
    }

    // If server URL or token changed, re-register
    if url_changed || token_changed {
        let mut ha_client = state.ha_client.lock().await;
        ha_client.update_config(server_url, access_token);

        // Clear registration status - will re-register on next cycle
        if settings.webhook_id.is_some() {
            settings.webhook_id = None;
            *state.is_registered.lock().await = false;
            if let Err(e) = settings.save(&app) {
                log::error!("[HA] Save settings failed: {}", e);
                return Err(e);
            }
        }
    }

    Ok(())
}

/// Register device with HA
#[tauri::command]
pub async fn register_device(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let mut settings = state.settings.lock().await;
    let mut ha_client = state.ha_client.lock().await;
    let mut collector = state.collector.lock().await;

    let webhook_id = match crate::registration::register_device(
        &mut settings,
        &mut ha_client,
        &mut collector,
        &app,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            log::error!("[HA] Registration failed: {}", e);
            return Err(e);
        }
    };

    *state.is_registered.lock().await = true;

    Ok(webhook_id)
}

/// Get list of all sensors
#[tauri::command]
pub async fn get_sensor_list(state: State<'_, Arc<AppState>>) -> Result<Vec<SensorListItem>, String> {
    let collector = state.collector.lock().await;
    Ok(collector.get_sensor_list())
}

/// Force immediate sensor update
#[tauri::command]
pub async fn update_sensors_now(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let is_registered = *state.is_registered.lock().await;
    if !is_registered {
        log::error!("[HA] update_sensors_now: device not registered");
        return Err("Device not registered".to_string());
    }

    let sensor_data = {
        let mut collector = state.collector.lock().await;
        collector.collect_dynamic()
    };

    let ha_client = state.ha_client.lock().await;
    if let Err(e) = ha_client.update_sensors(&sensor_data).await {
        log::error!("[HA] Update sensors failed: {}", e);
        return Err(format!("Update failed: {}", e));
    }

    Ok(())
}

/// Toggle a sensor on/off
#[tauri::command]
pub async fn toggle_sensor(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    sensor_id: String,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = state.settings.lock().await;
    settings.enabled_sensors.insert(sensor_id, enabled);
    if let Err(e) = settings.save(&app) {
        log::error!("[HA] Save settings failed: {}", e);
        return Err(e);
    }

    // Update collector
    let mut collector = state.collector.lock().await;
    collector.set_enabled_sensors(settings.enabled_sensors.clone());

    Ok(())
}

/// Get current language
#[tauri::command]
pub async fn get_current_language(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let settings = state.settings.lock().await;
    Ok(settings.language.clone())
}

/// Open the HA dashboard as a child webview inside the main window.
/// Only injects hassTokens in localStorage (no externalApp, which would
/// hijack the auth flow and break it for long-lived tokens).
pub fn open_dashboard_view<R: tauri::Runtime, M: Manager<R>>(
    manager: &M,
    server_url: &str,
    token: &str,
) -> Result<(), String> {
    let base_url = server_url.trim_end_matches('/');
    log::info!("[Dashboard] Opening dashboard view for: {}", base_url);

    // Close existing HA child webview if any
    if let Some(existing) = manager.get_webview("ha-view") {
        log::info!("[Dashboard] Closing existing ha-view");
        let _ = existing.close();
    }

    let escaped_token = token
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let escaped_url = base_url
        .replace('\\', "\\\\")
        .replace('"', "\\\"");

    // Initialization script: set hassTokens in localStorage BEFORE HA frontend loads.
    // Do NOT set window.externalApp — it hijacks auth and breaks long-lived tokens.
    let init_script = format!(
        r#"
        (function() {{
            try {{
                localStorage.setItem("hassTokens", JSON.stringify({{
                    hassUrl: "{escaped_url}",
                    access_token: "{escaped_token}",
                    token_type: "Bearer",
                    expires_in: 315360000,
                    refresh_token: "",
                    expires: Date.now() + 315360000000
                }}));
            }} catch(e) {{
                console.warn("[HA Companion] Failed to inject hassTokens:", e);
            }}
        }})();
        "#
    );

    let url: url::Url = base_url
        .parse()
        .map_err(|e: url::ParseError| format!("Invalid URL '{}': {}", base_url, e))?;

    let window = manager.get_window("main").ok_or("Main window not found")?;
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let phys = window.inner_size().map_err(|e| e.to_string())?;
    let logical = phys.to_logical::<f64>(scale);

    log::info!("[Dashboard] Adding child webview {}x{}", logical.width, logical.height);

    window
        .add_child(
            tauri::webview::WebviewBuilder::new("ha-view", tauri::WebviewUrl::External(url))
                .initialization_script(&init_script)
                .auto_resize(),
            tauri::LogicalPosition::new(0.0, 0.0),
            logical,
        )
        .map_err(|e| format!("Failed to create HA webview: {}", e))?;

    log::info!("[Dashboard] Dashboard view created");
    Ok(())
}

/// Remove the HA dashboard child webview (to reveal the main HTML underneath).
pub fn close_dashboard_view<R: tauri::Runtime, M: Manager<R>>(manager: &M) {
    if let Some(wv) = manager.get_webview("ha-view") {
        let _ = wv.close();
        log::info!("[Dashboard] Closed ha-view");
    }
}

/// Tauri command: open (or re-open) the HA dashboard view
#[tauri::command]
pub async fn load_dashboard(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let settings = state.settings.lock().await;
    let server_url = settings.server_url.clone();
    let token = settings.access_token.clone();
    drop(settings);

    open_dashboard_view(&app, &server_url, &token)
}

/// Tauri command: close the HA dashboard view (used when opening settings)
#[tauri::command]
pub async fn hide_dashboard(app: tauri::AppHandle) -> Result<(), String> {
    close_dashboard_view(&app);
    Ok(())
}

/// Get this machine's public (outbound) IP. Use this in your reverse proxy allowlist.
#[tauri::command]
pub async fn get_my_public_ip() -> Result<String, String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    let body = client
        .get("https://api.ipify.org")
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;
    Ok(body.trim().to_string())
}
