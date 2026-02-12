/**
 * Main frontend logic for Home Assistant Companion
 * Handles app initialization, setup form, tray events
 */

/**
 * Show the setup screen
 */
function showSetupScreen() {
    document.getElementById("setup-screen").classList.remove("hidden");
}

/**
 * Hide the setup screen
 */
function hideSetupScreen() {
    document.getElementById("setup-screen").classList.add("hidden");
}

/**
 * Handle initial setup form
 */
async function handleSetup(e) {
    e.preventDefault();

    const serverUrl = document.getElementById("setup-server-url").value.trim();
    const token = document.getElementById("setup-token").value.trim();
    const errorEl = document.getElementById("setup-error");
    const loadingEl = document.getElementById("setup-loading");

    errorEl.classList.add("hidden");
    loadingEl.classList.remove("hidden");

    try {
        // Save settings first
        await window.__TAURI__.core.invoke("save_settings", {
            serverUrl: serverUrl,
            accessToken: token,
            updateInterval: 60,
            language: currentLanguage,
            autostart: false,
        });

        // Register device
        await window.__TAURI__.core.invoke("register_device");

        // Success — open HA dashboard as child webview overlay
        hideSetupScreen();
        await window.__TAURI__.core.invoke("load_dashboard");
    } catch (err) {
        errorEl.textContent = err.toString();
        errorEl.classList.remove("hidden");
    } finally {
        loadingEl.classList.add("hidden");
    }
}

/**
 * Initialize the app
 */
async function initApp() {
    try {
        // Get current settings
        const settings = await window.__TAURI__.core.invoke("get_settings");

        // Set language
        setLanguage(settings.language || "en");

        if (!settings.server_url || !settings.access_token) {
            // No config — show setup wizard
            showSetupScreen();
        } else if (!settings.is_registered) {
            // Has config but not registered — show setup with pre-filled values
            showSetupScreen();
            document.getElementById("setup-server-url").value = settings.server_url;
            document.getElementById("setup-token").value = settings.access_token;
        } else {
            // Already registered — open HA dashboard as child webview
            hideSetupScreen();
            await window.__TAURI__.core.invoke("load_dashboard");
        }
    } catch (err) {
        console.error("Failed to initialize app:", err);
        showSetupScreen();
    }
}

// Event listeners
document.addEventListener("DOMContentLoaded", () => {
    // Setup form
    document.getElementById("setup-form").addEventListener("submit", handleSetup);

    // Listen for tray events
    if (window.__TAURI__) {
        window.__TAURI__.event.listen("tray-show-settings", () => {
            openSettings();
        });
    }

    // Initialize
    initApp();
});
