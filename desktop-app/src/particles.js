// HA-style background particles: same config as Home Assistant's
// src/resources/particles.ts (links preset, 50 dots, HA primary color).
// Bundle is loaded from ../vendor/ so we don't depend on a CDN at runtime.
(async function () {
    if (typeof window.tsParticles === "undefined") {
        console.warn("[particles] tsParticles bundle not loaded");
        return;
    }

    // The UMD bundle ships the preset but doesn't auto-register it — we
    // mirror HA's particles.ts by calling loadLinksPreset() explicitly.
    if (typeof window.loadLinksPreset === "function") {
        try { await window.loadLinksPreset(window.tsParticles); }
        catch (e) { console.warn("[particles] loadLinksPreset failed:", e); }
    }

    // HA's default primary color (used as both dot and link color).
    var PRIMARY = "#03a9f4";

    window.tsParticles.load({
        id: "tsparticles",
        options: {
            preset: "links",
            background: { opacity: 0 },
            fullScreen: { enable: true, zIndex: -1 },
            detectRetina: true,
            fpsLimit: 60,
            motion: {
                disable: false,
                reduce: { factor: 4, value: true },
            },
            particles: {
                color: { value: PRIMARY },
                links: {
                    color: { value: PRIMARY },
                    distance: 100,
                    enable: true,
                    frequency: 1,
                    opacity: 0.7,
                    width: 1,
                },
                move: { enable: true, speed: 0.5 },
                number: { value: 50 },
                opacity: {
                    value: { min: 0.3, max: 0.5 },
                    animation: {
                        destroy: "none",
                        enable: true,
                        speed: 0.5,
                        startValue: "random",
                        sync: false,
                    },
                },
                size: {
                    // Smaller than HA's 1-3px because WebView2 on Windows
                    // multiplies by display scaling (typically 1.25-1.5x),
                    // making HA's defaults visibly chunky. 0.5-1.5 lands at
                    // roughly the same on-screen size as HA's web build.
                    value: { min: 0.5, max: 1.5 },
                    animation: {
                        destroy: "none",
                        enable: true,
                        speed: 3,
                        startValue: "random",
                        sync: false,
                    },
                },
            },
            pauseOnBlur: true,
        },
    });
})();
