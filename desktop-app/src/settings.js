/**
 * Settings modal logic for Home Assistant Companion
 */

let currentSettings = null;

/**
 * Open settings modal and populate with current values
 */
async function openSettings() {
    try {
        currentSettings = await window.__TAURI__.core.invoke("get_settings");

        // Populate fields
        document.getElementById("settings-server-url").value = currentSettings.server_url || "";
        document.getElementById("settings-token").value = currentSettings.access_token || "";
        document.getElementById("settings-interval").value = currentSettings.update_interval || 60;
        document.getElementById("settings-language").value = currentSettings.language || "en";
        document.getElementById("settings-autostart").checked = currentSettings.autostart || false;

        // Device info
        document.getElementById("info-device-id").textContent = currentSettings.device_id || "-";
        document.getElementById("info-webhook-id").textContent = currentSettings.webhook_id
            ? currentSettings.webhook_id.substring(0, 16) + "..."
            : "-";
        document.getElementById("info-status").textContent = currentSettings.is_registered
            ? t("registered")
            : t("not_registered");
        document.getElementById("info-status").className =
            "info-value " + (currentSettings.is_registered ? "status-ok" : "status-error");

        // Populate sensor list
        await populateSensorList();

        // Show modal
        document.getElementById("settings-overlay").classList.remove("hidden");
    } catch (err) {
        console.error("Failed to load settings:", err);
    }
}

/**
 * Close settings modal
 */
function closeSettings() {
    document.getElementById("settings-overlay").classList.add("hidden");
}

/**
 * Save settings
 */
async function saveSettings() {
    const serverUrl = document.getElementById("settings-server-url").value.trim();
    const token = document.getElementById("settings-token").value.trim();
    const interval = parseInt(document.getElementById("settings-interval").value) || 60;
    const language = document.getElementById("settings-language").value;
    const autostart = document.getElementById("settings-autostart").checked;

    try {
        await window.__TAURI__.core.invoke("save_settings", {
            serverUrl: serverUrl,
            accessToken: token,
            updateInterval: interval,
            language: language,
            autostart: autostart,
        });

        // Update language
        setLanguage(language);

        // Reload dashboard if URL changed
        if (currentSettings && serverUrl !== currentSettings.server_url) {
            loadDashboard(serverUrl, token);
        }

        closeSettings();
    } catch (err) {
        console.error("Failed to save settings:", err);
        alert("Failed to save settings: " + err);
    }
}

/**
 * Populate sensor list with checkboxes
 */
async function populateSensorList() {
    try {
        const sensors = await window.__TAURI__.core.invoke("get_sensor_list");
        const container = document.getElementById("sensor-list");
        container.innerHTML = "";

        for (const sensor of sensors) {
            const row = document.createElement("div");
            row.className = "sensor-row";

            const checkbox = document.createElement("input");
            checkbox.type = "checkbox";
            checkbox.id = `sensor-${sensor.id}`;
            checkbox.checked = sensor.enabled;
            checkbox.addEventListener("change", async () => {
                try {
                    await window.__TAURI__.core.invoke("toggle_sensor", {
                        sensorId: sensor.id,
                        enabled: checkbox.checked,
                    });
                } catch (err) {
                    console.error("Failed to toggle sensor:", err);
                    checkbox.checked = !checkbox.checked;
                }
            });

            const label = document.createElement("label");
            label.htmlFor = `sensor-${sensor.id}`;
            label.textContent = t(sensor.id) || sensor.name;

            const badge = document.createElement("span");
            badge.className = "sensor-badge " + (sensor.updates_at_interval ? "badge-dynamic" : "badge-static");
            badge.textContent = sensor.updates_at_interval ? t("updates_at_interval") : t("static_sensor");

            row.appendChild(checkbox);
            row.appendChild(label);
            row.appendChild(badge);
            container.appendChild(row);
        }
    } catch (err) {
        console.error("Failed to load sensor list:", err);
    }
}

/**
 * Toggle password visibility
 */
function togglePassword(inputId) {
    const input = document.getElementById(inputId);
    input.type = input.type === "password" ? "text" : "password";
}

// Event listeners
document.addEventListener("DOMContentLoaded", () => {
    document.getElementById("settings-close").addEventListener("click", closeSettings);
    document.getElementById("settings-cancel").addEventListener("click", closeSettings);
    document.getElementById("settings-save").addEventListener("click", saveSettings);

    // Close on overlay click
    document.getElementById("settings-overlay").addEventListener("click", (e) => {
        if (e.target === document.getElementById("settings-overlay")) {
            closeSettings();
        }
    });

    // Close on Escape
    document.addEventListener("keydown", (e) => {
        if (e.key === "Escape") {
            closeSettings();
        }
    });
});
