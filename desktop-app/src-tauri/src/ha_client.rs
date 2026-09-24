use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::sensors::collector::SensorValue;

/// Normalize server URL: trim whitespace and strip trailing /api so we never build double /api/api/ paths.
pub fn normalize_server_url(url: &str) -> String {
    let s = url.trim().trim_end_matches('/');
    s.strip_suffix("/api")
        .map(|u| u.trim_end_matches('/'))
        .unwrap_or(s)
        .to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationRequest {
    pub device_id: String,
    pub device_name: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub app_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationResponse {
    pub success: bool,
    pub webhook_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct WebhookPayload {
    protocol_version: u8,
    #[serde(rename = "type")]
    command_type: String,
    data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
struct SensorRegistration {
    #[serde(flatten)]
    metadata: SensorMetadata,
    sensor_state: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SensorMetadata {
    sensor_unique_id: String,
    sensor_name: String,
    sensor_type: String,
    sensor_device_class: Option<String>,
    sensor_unit_of_measurement: Option<String>,
    sensor_state_class: Option<String>,
    sensor_icon: Option<String>,
    update_at_interval: bool,
}

impl From<&SensorValue> for SensorMetadata {
    fn from(sensor: &SensorValue) -> Self {
        Self {
            sensor_unique_id: sensor.unique_id.clone(),
            sensor_name: sensor.name.clone(),
            sensor_type: sensor.sensor_type.clone(),
            sensor_device_class: sensor.device_class.clone(),
            sensor_unit_of_measurement: sensor.unit_of_measurement.clone(),
            sensor_state_class: sensor.state_class.clone(),
            sensor_icon: sensor.icon.clone(),
            update_at_interval: sensor.update_at_interval,
        }
    }
}

fn sensor_descriptors(sensors: &[SensorValue]) -> Vec<SensorMetadata> {
    let mut descriptors: Vec<_> = sensors.iter().map(SensorMetadata::from).collect();
    descriptors.sort_unstable_by(|left, right| left.sensor_unique_id.cmp(&right.sensor_unique_id));
    descriptors
}

fn changed_sensors<'a>(
    sensors: &'a [SensorValue],
    registered: Option<&[SensorMetadata]>,
) -> Vec<&'a SensorValue> {
    sensors
        .iter()
        .filter(|sensor| {
            let descriptor = SensorMetadata::from(*sensor);
            !registered.is_some_and(|known| {
                known
                    .binary_search_by(|item| {
                        item.sensor_unique_id.cmp(&descriptor.sensor_unique_id)
                    })
                    .ok()
                    .is_some_and(|index| known[index] == descriptor)
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
struct SensorStateUpdate {
    sensor_unique_id: String,
    sensor_state: serde_json::Value,
    sensor_attributes: serde_json::Value,
    sensor_icon: Option<String>,
}

fn registration_payload(sensor: &SensorValue) -> serde_json::Result<WebhookPayload> {
    Ok(WebhookPayload {
        protocol_version: 1,
        command_type: "register_sensor".to_string(),
        data: serde_json::to_value(SensorRegistration {
            metadata: SensorMetadata::from(sensor),
            sensor_state: sensor.state.clone(),
        })?,
    })
}

fn sensor_update_payload(
    sensors: &[SensorValue],
    update_interval: u64,
    snapshot_scope: &str,
) -> WebhookPayload {
    let sensor_updates: Vec<SensorStateUpdate> = sensors
        .iter()
        .map(|sensor| SensorStateUpdate {
            sensor_unique_id: sensor.unique_id.clone(),
            sensor_state: sensor.state.clone(),
            sensor_attributes: serde_json::to_value(&sensor.attributes).unwrap_or_default(),
            sensor_icon: sensor.icon.clone(),
        })
        .collect();
    WebhookPayload {
        protocol_version: 1,
        command_type: "update_sensor_states".to_string(),
        data: serde_json::json!({
            "sensors": sensor_updates,
            "update_interval": update_interval,
            "snapshot_scope": snapshot_scope,
        }),
    }
}

pub struct HaClient {
    client: Client,
    server_url: String,
    access_token: String,
    webhook_id: Option<String>,
    update_interval: u64,
    registered_descriptors: Option<Vec<SensorMetadata>>,
}

impl HaClient {
    fn webhook_transport_error(error: reqwest::Error) -> String {
        if error.is_timeout() {
            "Webhook request timed out".into()
        } else if error.is_connect() {
            "Webhook connection failed".into()
        } else {
            "Webhook transport failed".into()
        }
    }

    async fn require_webhook_ack(
        response: reqwest::Response,
        command: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let status = response.status();
        if !status.is_success() {
            return Err(format!("{command} returned HTTP {status}").into());
        }
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|_| format!("{command} did not return a JSON acknowledgement"))?;
        if body.get("success") != Some(&serde_json::Value::Bool(true)) {
            return Err(format!("{command} was not acknowledged by Home Assistant").into());
        }
        if body.get("protocol_version").is_some()
            && body.get("protocol_version") != Some(&serde_json::json!(1))
        {
            return Err(format!("{command} returned an unsupported protocol version").into());
        }
        Ok(())
    }

    pub fn new(server_url: String, access_token: String, webhook_id: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            client,
            server_url: normalize_server_url(&server_url),
            access_token: access_token.trim().to_string(),
            webhook_id,
            update_interval: 60,
            registered_descriptors: None,
        }
    }

    pub fn update_config(&mut self, server_url: String, access_token: String) {
        let server_url = normalize_server_url(&server_url);
        let access_token = access_token.trim().to_string();
        if self.server_url != server_url || self.access_token != access_token {
            self.registered_descriptors = None;
        }
        self.server_url = server_url;
        self.access_token = access_token;
    }

    /// Base URL for API calls (no trailing slash, no trailing /api)
    fn base_url(&self) -> &str {
        self.server_url.trim_end_matches('/')
    }

    /// Check if the Desktop App integration is reachable (GET /api/desktop_app/ping, no auth).
    /// Returns Ok(()) if reachable, Err with message if 404 or connection failed.
    pub async fn check_integration_reachable(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/desktop_app/ping", self.base_url());
        log::info!("[HA] Checking integration ping endpoint");
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;
        let status = response.status();
        log::info!("[HA] ping response: {}", status);
        if status.as_u16() == 404 {
            let msg = "404: Desktop App integration not loaded or URL not reachable. \
                Install the integration in HA, restart HA, and ensure the server URL is correct (base URL without /api). \
                If using a reverse proxy, ensure /api/ is forwarded to Home Assistant.";
            log::error!("[HA] Ping failed: {}", msg);
            return Err(msg.into());
        }
        if !response.status().is_success() {
            let err = format!("Integration ping returned {}", response.status());
            log::error!("[HA] {}", err);
            return Err(err.into());
        }
        Ok(())
    }

    pub fn set_webhook_id(&mut self, webhook_id: String) {
        if self.webhook_id.as_deref() != Some(webhook_id.as_str()) {
            self.registered_descriptors = None;
        }
        self.webhook_id = Some(webhook_id);
    }

    pub fn set_update_interval(&mut self, seconds: u64) {
        self.update_interval = seconds.clamp(5, 3600);
    }

    pub fn clear_webhook_id(&mut self) {
        self.webhook_id = None;
        self.registered_descriptors = None;
    }

    pub fn webhook_id(&self) -> Option<&str> {
        self.webhook_id.as_deref()
    }

    pub async fn update_registration(
        &self,
        registration: &RegistrationRequest,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let webhook_id = self.webhook_id.as_ref().ok_or("No webhook_id configured")?;
        let url = format!("{}/api/webhook/{}", self.base_url(), webhook_id);
        let payload = WebhookPayload {
            protocol_version: 1,
            command_type: "update_registration".into(),
            data: serde_json::json!({
                "device_name": registration.device_name,
                "os_version": registration.os_version,
                "app_version": registration.app_version,
            }),
        };
        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;
        Self::require_webhook_ack(response, "update_registration").await
    }

    /// Register device with HA
    pub async fn register_device(
        &self,
        registration: &RegistrationRequest,
    ) -> Result<RegistrationResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/desktop_app/registrations", self.base_url());
        log::info!("[HA] Registering device");

        let response = self
            .client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.access_token.trim()),
            )
            .header("Content-Type", "application/json")
            .json(registration)
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        log::info!(
            "[HA] registration response: {} body_len={}",
            status,
            body.len()
        );
        if !status.is_success() {
            if status.as_u16() == 404 {
                let msg = "404 Not Found: Desktop App integration not loaded or URL not reachable.";
                log::error!("[HA] Registration {}", msg);
                return Err(
                    "404 Not Found: Desktop App integration not loaded or URL not reachable. \
                    Check: (1) Integration installed in HA and HA restarted, (2) Server URL is the HA base URL without /api, (3) Reverse proxy forwards /api/ to HA."
                        .into(),
                );
            }
            if status.as_u16() == 401 {
                log::error!("[HA] Registration 401 Unauthorized");
                return Err("401 Unauthorized: Invalid or expired access token.".into());
            }
            log::error!("[HA] Registration failed {}", status);
            return Err(format!("Registration failed ({status})").into());
        }

        let result: RegistrationResponse = serde_json::from_str(&body).map_err(|e| {
            let err = format!("Invalid JSON registration response: {e}");
            log::error!("[HA] {}", err);
            err
        })?;
        Ok(result)
    }

    /// Register a single sensor with HA
    pub async fn register_sensor(
        &self,
        sensor: &SensorValue,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let webhook_id = self.webhook_id.as_ref().ok_or("No webhook_id configured")?;

        let url = format!("{}/api/webhook/{}", self.base_url(), webhook_id);

        let payload = registration_payload(sensor)?;

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;

        Self::require_webhook_ack(response, "register_sensor").await
    }

    /// Register multiple sensors with HA
    pub async fn register_sensors(
        &self,
        sensors: &[SensorValue],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for sensor in sensors {
            self.register_sensor(sensor).await?;
        }
        Ok(())
    }

    pub async fn register_sensors_if_changed(
        &mut self,
        sensors: &[SensorValue],
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let descriptors = sensor_descriptors(sensors);
        if self.registered_descriptors.as_ref() == Some(&descriptors) {
            return Ok(false);
        }
        for sensor in changed_sensors(sensors, self.registered_descriptors.as_deref()) {
            self.register_sensor(sensor).await?;
        }
        self.registered_descriptors = Some(descriptors);
        Ok(true)
    }

    pub fn remember_registered_sensors(&mut self, sensors: &[SensorValue]) {
        self.registered_descriptors = Some(sensor_descriptors(sensors));
    }

    pub fn forget_registered_sensors(&mut self) {
        self.registered_descriptors = None;
    }

    /// Batch update sensor states
    pub async fn update_sensors(
        &self,
        sensors: &[SensorValue],
        snapshot_scope: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let webhook_id = self.webhook_id.as_ref().ok_or("No webhook_id configured")?;

        let url = format!("{}/api/webhook/{}", self.base_url(), webhook_id);

        let payload = sensor_update_payload(sensors, self.update_interval, snapshot_scope);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;

        Self::require_webhook_ack(response, "update_sensor_states").await
    }

    /// Tell HA that this device is shutting down. Best-effort; the caller
    /// must time-bound the await (e.g. with tokio::time::timeout) because
    /// the OS may have already started reclaiming sockets when this runs.
    pub async fn send_device_offline(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let webhook_id = self.webhook_id.as_ref().ok_or("No webhook_id configured")?;
        let url = format!("{}/api/webhook/{}", self.base_url(), webhook_id);
        let payload = WebhookPayload {
            protocol_version: 1,
            command_type: "device_offline".to_string(),
            data: serde_json::json!({}),
        };
        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;
        Self::require_webhook_ack(response, "device_offline").await
    }

    /// Check if the webhook is still valid
    pub async fn check_webhook(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let webhook_id = match &self.webhook_id {
            Some(id) => id,
            None => return Err("No webhook_id configured".into()),
        };

        let url = format!("{}/api/webhook/{}", self.base_url(), webhook_id);

        // Send a minimal payload to check if webhook exists
        let payload = WebhookPayload {
            protocol_version: 1,
            command_type: "update_sensor_states".to_string(),
            data: serde_json::json!({"sensors": [], "update_interval": self.update_interval}),
        };

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(Self::webhook_transport_error)?;
        Self::require_webhook_ack(response, "webhook health check").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn desktop_wire_payloads_match_shared_protocol_fixture() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../contracts/protocol-v1.json")).unwrap();
        let data = &fixture["register_sensor"]["data"];
        let mut sensor = SensorValue {
            unique_id: data["sensor_unique_id"].as_str().unwrap().into(),
            name: data["sensor_name"].as_str().unwrap().into(),
            state: data["sensor_state"].clone(),
            sensor_type: data["sensor_type"].as_str().unwrap().into(),
            device_class: None,
            unit_of_measurement: Some("%".into()),
            state_class: Some("measurement".into()),
            icon: Some("mdi:cpu-64-bit".into()),
            attributes: HashMap::new(),
            update_at_interval: true,
        };
        assert_eq!(
            serde_json::to_value(registration_payload(&sensor).unwrap()).unwrap(),
            fixture["register_sensor"]
        );

        sensor.state = serde_json::json!(21.5);
        sensor
            .attributes
            .insert("measurement_source".into(), serde_json::json!("system"));
        assert_eq!(
            serde_json::to_value(sensor_update_payload(&[sensor], 60, "all")).unwrap(),
            fixture["update_sensor_states"]
        );
    }

    #[test]
    fn device_offline_payload_serializes_with_expected_shape() {
        let payload = WebhookPayload {
            protocol_version: 1,
            command_type: "device_offline".to_string(),
            data: serde_json::json!({}),
        };
        let s = serde_json::to_string(&payload).expect("serialize");
        assert!(s.contains(r#""type":"device_offline""#), "payload was: {s}");
        assert!(s.contains(r#""data":{}"#), "payload was: {s}");
        assert!(s.contains(r#""protocol_version":1"#), "payload was: {s}");
    }

    #[test]
    fn registration_cache_ignores_measurements_but_detects_metadata_and_webhook_changes() {
        let mut sensor = SensorValue {
            unique_id: "cpu_usage".into(),
            name: "CPU Usage".into(),
            state: serde_json::json!(15),
            sensor_type: "sensor".into(),
            device_class: None,
            unit_of_measurement: Some("%".into()),
            state_class: Some("measurement".into()),
            icon: Some("mdi:cpu-64-bit".into()),
            attributes: HashMap::new(),
            update_at_interval: true,
        };
        let mut client = HaClient::new(
            "https://ha.example".into(),
            "token".into(),
            Some("old".into()),
        );
        client.remember_registered_sensors(&[sensor.clone()]);
        sensor.state = serde_json::json!(42);
        sensor
            .attributes
            .insert("source".into(), serde_json::json!("new"));
        assert_eq!(
            client.registered_descriptors,
            Some(sensor_descriptors(&[sensor.clone()]))
        );
        assert!(
            changed_sensors(&[sensor.clone()], client.registered_descriptors.as_deref()).is_empty()
        );

        sensor.name = "Processor Usage".into();
        assert_eq!(
            changed_sensors(&[sensor.clone()], client.registered_descriptors.as_deref()).len(),
            1
        );
        assert_ne!(
            client.registered_descriptors,
            Some(sensor_descriptors(&[sensor]))
        );
        client.set_webhook_id("new".into());
        assert!(client.registered_descriptors.is_none());
    }

    #[tokio::test]
    async fn webhook_health_requires_a_real_acknowledgement() {
        for (status, body, accepted) in [
            ("200 OK", "", false),
            ("200 OK", r#"{"success":false}"#, false),
            ("200 OK", r#"{"success":true}"#, true),
            ("200 OK", r#"{"success":true,"protocol_version":1}"#, true),
            ("200 OK", r#"{"success":true,"protocol_version":2}"#, false),
            ("410 Gone", r#"{"success":false}"#, false),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let reply = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 2048];
                let length = socket.read(&mut request).await.unwrap();
                assert!(
                    String::from_utf8_lossy(&request[..length]).contains("/api/webhook/test-id")
                );
                socket.write_all(reply.as_bytes()).await.unwrap();
            });
            let client = HaClient::new(
                format!("http://{address}"),
                "test-token".into(),
                Some("test-id".into()),
            );
            let result = client.check_webhook().await;
            assert_eq!(result.is_ok(), accepted, "status={status}, body={body}");
            server.await.unwrap();
        }
    }
}
