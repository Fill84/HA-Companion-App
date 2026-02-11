/**
 * Main frontend logic for Home Assistant Companion
 * Handles app initialization, dashboard loading, tray events, and auto-login
 */

/**
 * Load the HA dashboard in the iframe with auto-login
 */
function loadDashboard(serverUrl, token) {
    const iframe = document.getElementById("ha-dashboard");
    const container = document.getElementById("dashboard-container");

    if (!serverUrl || !token) {
        container.classList.add("hidden");
        return;
    }

    // Clean up the URL
    const baseUrl = serverUrl.replace(/\/+$/, "");

    // Strategy: Load a blank page first, inject hassTokens into localStorage,
    // then navigate to the HA dashboard
    iframe.src = "about:blank";

    iframe.onload = function onFirstLoad() {
        iframe.onload = null; // Remove this handler

        try {
            // Inject hassTokens into the iframe's localStorage
            const hassTokens = {
                hassUrl: baseUrl,
                access_token: token,
                token_type: "Bearer",
            };

            iframe.contentWindow.localStorage.setItem(
                "hassTokens",
                JSON.stringify(hassTokens)
            );
        } catch (e) {
            console.warn("Could not inject tokens into iframe localStorage:", e);
        }

        // Now navigate to the actual HA dashboard
        iframe.src = baseUrl;
        container.classList.remove("hidden");
    };
}

/**
 * Show the setup screen
 */
function showSetupScreen() {
    document.getElementById("setup-screen").classList.remove("hidden");
    document.getElementById("dashboard-container").classList.add("hidden");
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

        // Success — load dashboard
        hideSetupScreen();
        loadDashboard(serverUrl, token);
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
            // Has config but not registered — try to register
            showSetupScreen();
            document.getElementById("setup-server-url").value = settings.server_url;
            document.getElementById("setup-token").value = settings.access_token;
        } else {
            // Already configured and registered — load dashboard
            loadDashboard(settings.server_url, settings.access_token);
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
