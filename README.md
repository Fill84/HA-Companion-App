# Home Assistant Companion App

Een cross-platform desktop companion app voor Home Assistant, inclusief een custom integratie voor HACS.

## Features
- **Desktop App (Tauri, Rust, JS):**
  - Systeeminformatie verzamelen (CPU, GPU, RAM, disk, netwerk, batterij, OS, BIOS, moederbord)
  - Automatische registratie en sensor updates via webhooks naar Home Assistant
  - Native system tray met context menu (Tonen/Verbergen, Instellingen, Afsluiten)
  - Auto-login in HA dashboard via access token injectie
  - Instelbare taal (EN/NL), settings modal, sensor enable/disable
  - Beoogde release-artifacts: Windows (.exe/.msi), macOS (.dmg), Linux (.deb/.rpm/.AppImage); de platformmatrix is nog niet volledig gevalideerd

- **Home Assistant Integratie:**
  - Custom component voor device registry, dynamische sensors, webhook-based updates
  - HACS support via de afzonderlijke `ha-integration`-repository
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

## Bouw en releasecontrole
De desktop gebruikt Yarn 1.22.22 met één `yarn.lock`; een schone installatie is `cd desktop-app && corepack yarn install --frozen-lockfile`. Pull requests en pushes naar `main` draaien de desktopverificatie. Een release start uitsluitend via `workflow_dispatch` op `main` met de volledige, gepushte 40-tekens commit-SHA uit de afzonderlijke `Fill84/ha-integration`-repository. `package.json`, `Cargo.toml` en `tauri.conf.json` moeten dezelfde desktopversie bevatten; de release bevat een manifest met beide broncommits en SHA-256-hashes van alle artifacts. De daadwerkelijke platform-, installer- en HA-compatibiliteitsproeven zijn nog releasevoorwaarden. Zie de [uitvoeringsstatus](docs/plans/2026-09-24-verificatiestatus.md).

## Projectstructuur
```
HA-Companion-App/
├── desktop-app/        # Tauri desktop app (Rust + JS)
│   ├── src/
│   ├── src-tauri/
│   └── ...
├── ha-integration/     # Lokale aparte Git-repo (genegeerd door hoofdrepo)
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

## CPU-temperatuur en hostbelasting

De basisapp bevat geen WinRing0-driver meer. CPU-gebruik, frequentie en model werken zonder een extra temperatuurdriver. Voor Windows-CPU-temperatuur is een expliciet inschakelbare provider beschikbaar; deze vereist apart .NET 10 en de officiële PawnIO-driver. Zonder die provider is de temperatuur `unknown`. De app installeert of beheert geen kernel-driver.

De collector levert één snapshot per meetronde en hergebruikt de basis-systeemmetingen. De optionele helper blijft actief tijdens gebruik en leest op verzoek. Zie [providerdocumentatie, bouwinstructies en migratiegrenzen](hwmon-helper/README.md) en de [sensor-supportmatrix](docs/specs/2026-09-24-sensor-support.md). Bestaande WinRing0-services worden niet automatisch verwijderd; een upgrade van een oude installatie vraagt nog migratievalidatie.

## License
Zie LICENSE.md voor licentievoorwaarden.

## Contact & Support
Voor vragen, issues of feature requests: open een issue op GitHub.

---

**Made with ❤️ for Home Assistant users!**
