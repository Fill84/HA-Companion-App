# Home Assistant Companion App

Een cross-platform desktop companion app voor Home Assistant, inclusief een custom integratie voor HACS.

## Features
- **Desktop App (Tauri, Rust, JS):**
  - Systeeminformatie verzamelen (CPU, GPU, RAM, disk, netwerk, batterij, OS, BIOS, moederbord)
  - Automatische registratie en sensor updates via webhooks naar Home Assistant
  - Native system tray met context menu (Tonen/Verbergen, Instellingen, Afsluiten)
  - Auto-login in HA dashboard via access token injectie
  - Instelbare taal (EN/NL), settings modal, sensor enable/disable
  - Cross-platform builds: Windows (.exe/.msi), macOS (.dmg), Linux (.deb/.rpm/.AppImage)

- **Home Assistant Integratie:**
  - Custom component voor device registry, dynamische sensors, webhook-based updates
  - HACS support via integratie branch (custom repo)
  - Volledige device info, sensor entities, binary sensors
  - UI strings en vertalingen (EN/NL)

## Installatie

### Desktop App
1. Download de laatste release voor jouw platform van [GitHub Releases](https://github.com/Fill84/HA-Companion-App/releases)
2. Installeer de app en start deze
3. Vul je Home Assistant server URL en Long-Lived Access Token in bij de eerste setup
4. De app registreert automatisch je device en sensors in Home Assistant

### Home Assistant Integratie (via HACS)
1. Voeg deze repo toe als custom repository in HACS:
   - Repository: `https://github.com/Fill84/ha-integration`
   - Type: `Integration`
   - (De integratie staat in `custom_components/desktop_app` in de repo-root zodat HACS hem vindt.)
2. Zoek "Desktop App" en installeer de integratie
3. Herstart Home Assistant
4. Configureer de integratie via Instellingen → Integraties

- voor meer informatie over de integratie bezoek: https://github.com/Fill84/ha-integration

## Automatische Releases
- GitHub Actions workflow bouwt en released installers voor alle platforms bij elke push naar main/integratie
- Versiebeheer en artifacts zijn volledig geautomatiseerd

## Projectstructuur
```
HA-Companion-App/
├── desktop-app/        # Tauri desktop app (Rust + JS)
│   ├── src/
│   ├── src-tauri/
│   └── ...
├── ha-integration/     # Home Assistant custom integratie
│   ├── custom_components/
│   └── ...
├── .github/workflows/  # Release workflow
├── README.md           # Dit bestand
├── LICENSE.md          # Licentie
└── ...
```

## Cross-platform
- Windows: NSIS installer (.exe), MSI
- macOS: DMG (.dmg)
- Linux: DEB (.deb), RPM (.rpm), AppImage

## License
Zie LICENSE.md voor licentievoorwaarden.

## Contact & Support
Voor vragen, issues of feature requests: open een issue op GitHub.

---

**Made with ❤️ for Home Assistant users!**