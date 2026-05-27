use crate::ha_client::{HaClient, RegistrationRequest};
use crate::sensors::collector::SensorCollector;
use crate::settings::AppSettings;

/// Perform full device registration with HA
pub async fn register_device(
    settings: &mut AppSettings,
    ha_client: &mut HaClient,
    collector: &mut SensorCollector,
    app_handle: &tauri::AppHandle,
) -> Result<String, String> {
    // Validate settings
    if settings.server_url.is_empty() {
        log::error!("[HA] Registration: server URL is empty");
        return Err("Server URL is not configured".to_string());
    }
    if settings.access_token.is_empty() {
        log::error!("[HA] Registration: access token is empty");
        return Err("Access token is not configured".to_string());
    }

    // Collect device metadata
    let sys_info = crate::sensors::system_info::collect();

    let registration = RegistrationRequest {
        device_id: settings.device_id.clone(),
        device_name: sys_info.hostname.clone(),
        manufacturer: sys_info.motherboard_manufacturer.clone(),
        model: sys_info.motherboard_model.clone(),
        os_name: Some(sys_info.os_name.clone()),
        os_version: Some(sys_info.os_version.clone()),
        app_version: Some(env!("CARGO_PKG_VERSION").to_string()),
    };

    // Check that the integration is reachable first (clearer 404 message)
    if let Err(e) = ha_client.check_integration_reachable().await {
        let msg = format!("Cannot reach Home Assistant Desktop App API. {}", e);
        log::error!("[HA] {}", msg);
        return Err(msg);
    }

    // Register device
    let response = ha_client
        .register_device(&registration)
        .await
        .map_err(|e| format!("Registration failed: {}", e))?;

    if !response.success {
        let err = format!(
            "Registration rejected: {}",
            response.error.unwrap_or_else(|| "Unknown error".to_string())
        );
        log::error!("[HA] {}", err);
        return Err(err);
    }

    let webhook_id = response.webhook_id.ok_or_else(|| {
        log::error!("[HA] Registration response missing webhook_id");
        "No webhook_id in response".to_string()
    })?;

    // Save webhook_id
    settings.webhook_id = Some(webhook_id.clone());
    ha_client.set_webhook_id(webhook_id.clone());
    if let Err(e) = settings.save(app_handle) {
        log::error!("[HA] Failed to save settings: {}", e);
        return Err(format!("Failed to save settings: {}", e));
    }

    // After config entry creation, HA still needs to run async_setup_entry
    // (which registers the webhook handler and starts the sensor platforms)
    // before our webhook POSTs will work. Instead of a fixed sleep we poll:
    // first sensor registration is retried with exponential backoff. On a
    // healthy HA this typically succeeds within 1-2 seconds; on a slow or
    // heavily loaded HA we keep trying for ~25s before giving up.
    let all_sensors = collector.collect_all(None);

    let first_sensor = all_sensors.first().cloned().ok_or_else(|| {
        log::error!("[HA] No sensors collected — cannot complete registration");
        "No sensors available to register".to_string()
    })?;

    let mut attempt: u32 = 0;
    let max_attempts: u32 = 8;
    let mut delay_ms: u64 = 500;
    loop {
        match ha_client.register_sensor(&first_sensor).await {
            Ok(()) => {
                log::info!(
                    "[HA] Initial sensor registered after {} retr{}",
                    attempt,
                    if attempt == 1 { "y" } else { "ies" }
                );
                break;
            }
            Err(e) => {
                attempt += 1;
                let err_str = e.to_string();
                // 410 from HA means webhook is known but config entry missing;
                // this won't resolve by waiting longer.
                if err_str.contains("410") {
                    log::error!(
                        "[HA] Initial sensor registration returned 410 — config entry missing"
                    );
                    return Err(format!("Sensor registration failed: {}", err_str));
                }
                if attempt >= max_attempts {
                    log::error!(
                        "[HA] Initial sensor registration failed after {} attempts: {}",
                        attempt,
                        err_str
                    );
                    return Err(format!("Sensor registration failed: {}", err_str));
                }
                log::warn!(
                    "[HA] Initial sensor registration attempt {} failed: {} (retrying in {}ms)",
                    attempt,
                    err_str,
                    delay_ms
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                // 500ms -> 1s -> 2s -> 4s -> 8s -> 8s -> 8s ...
                delay_ms = (delay_ms * 2).min(8000);
            }
        }
    }

    // Webhook is alive; register the remaining sensors. Skip the first one
    // (already registered above).
    if all_sensors.len() > 1 {
        if let Err(e) = ha_client.register_sensors(&all_sensors[1..]).await {
            log::error!("[HA] Remaining sensor registration failed: {}", e);
            return Err(format!("Sensor registration failed: {}", e));
        }
    }

    // Send initial sensor states
    if let Err(e) = ha_client.update_sensors(&all_sensors).await {
        log::error!("[HA] Initial sensor update failed: {}", e);
        return Err(format!("Initial sensor update failed: {}", e));
    }

    log::info!("Device registered successfully with webhook_id: {}", webhook_id);

    Ok(webhook_id)
}

/// Re-register device (when server URL or token changes)
#[allow(dead_code)]
pub async fn re_register(
    settings: &mut AppSettings,
    ha_client: &mut HaClient,
    collector: &mut SensorCollector,
    app_handle: &tauri::AppHandle,
) -> Result<String, String> {
    // Clear existing webhook_id
    settings.webhook_id = None;
    settings.save(app_handle).map_err(|e| format!("Failed to save settings: {}", e))?;

    // Update HA client
    ha_client.update_config(settings.server_url.clone(), settings.access_token.clone());

    // Perform fresh registration
    register_device(settings, ha_client, collector, app_handle).await
}
