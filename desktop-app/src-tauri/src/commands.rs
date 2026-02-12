use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

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
