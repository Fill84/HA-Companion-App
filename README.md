# Home Assistant Companion App

Een cross-platform desktop companion app voor Home Assistant, inclusief een custom integratie voor HACS.

## Features
- **Desktop App (Tauri, Rust, JS):**
  - Systeeminformatie verzamelen (CPU, GPU, RAM, disk, netwerk, batterij, OS, BIOS, moederbord)
  - Automatische registratie en sensor updates via webhooks naar Home Assistant
  - Native system tray met context menu (Tonen/Verbergen, Instellingen, Afsluiten)
  - Auto-login in HA dashboard via access token injectie
  - Instelbare taal (EN/NL), settings modal, sensor enable/disable
  - Release 1.0.5: Windows x64 NSIS, macOS Apple Silicon DMG en Linux x86_64 DEB/RPM/AppImage; de macOS-/Linux-runtime is nog niet interactief gevalideerd

- **Home Assistant Integratie:**
  - Custom component voor device registry, dynamische sensors, webhook-based updates
  - HACS support via de afzonderlijke `ha-integration`-repository
  - Volledige device info, sensor entities, binary sensors
  - UI strings en vertalingen (EN/NL)

## Installatie

### Desktop App
1. Download [release 1.0.5](https://github.com/Fill84/HA-Companion-App/releases/tag/1.0.5) voor jouw beschikbare platform en architectuur. Controleer versie en SHA-256 tegen `release-manifest.json`. Een lokaal `target/`-bestand is geen publieke release.
   De komende Windows-installer is naar verwachting ongetekend. Windows kan `Onbekende uitgever` of een SmartScreen-waarschuwing tonen; de meegeleverde driver heeft wel een eigen leveranciershandtekening. Controleer de exacte release en de SHA-256 voordat je de installer uitvoert.
2. Installeer de app en start deze
3. Vul je Home Assistant server URL en Long-Lived Access Token in bij de eerste setup
4. De app registreert automatisch je device en sensors in Home Assistant. Op Windows installeert onze installer de meegeleverde, ondertekende PawnIO-driver en onze eigen Rust-sensorservice standaard als onderdeel van de app. Een aparte sensorapp of extra schakelaar is niet nodig.

Het toegangstoken wordt in de Windows Credential Manager, macOS Keychain of een Linux Secret Service opgeslagen. Bij een upgrade wordt een eerder token uit `settings.json` alleen na succesvolle overdracht naar de OS-kluis uit dat bestand verwijderd. Als de kluis in de huidige sessie niet beschikbaar is, mislukt het opslaan met een foutmelding; op Linux is hiervoor een werkende Secret Service in de desktopsessie nodig. De afhandeling in een Windows-SSH-sessie is nog niet representatief voor een interactieve appstart en staat in de [verificatiestatus](docs/plans/2026-09-24-verificatiestatus.md).

### Home Assistant Integratie (via HACS)
1. Voeg deze repo toe als custom repository in HACS:
   - Repository: `https://github.com/Fill84/ha-integration`
   - Type: `Integration`
   - (De integratie staat in `custom_components/desktop_app` in de repo-root zodat HACS hem vindt.)
2. Zoek "Desktop App" en installeer de integratie
3. Herstart Home Assistant
4. Configureer de integratie via Instellingen → Integraties

- voor meer informatie over de integratie bezoek: https://github.com/Fill84/ha-integration

Lees de [volledige installatie-, upgrade-, dashboard- en herstelgids](https://github.com/Fill84/ha-integration/blob/main/docs/INSTALLATION.md) voordat je een bestaande installatie bijwerkt. Eerst de HA-integratie bijwerken en HA volledig herstarten, daarna de bijbehorende desktopversie installeren. HACS leest de afzonderlijke `ha-integration`-repository; een desktoprelease alleen levert geen HACS-update. Integratieversie 1.0.11 is gepubliceerd en via HACS op de live HA-installatie getest; daarbij bleven de bestaande entity-ID's behouden.

## Bouw en releasecontrole
De Windows-releaseworkflow controleert dat appbinary en NSIS-installer allebei geldig ondertekend of allebei ongetekend zijn. De gebruiker heeft distributie zonder publiek vertrouwde handtekening met zichtbare Windows-waarschuwing toegestaan. De status staat in het release-manifest en de releasenotities; zie [Windows-codeondertekening en releasebeleid](docs/RELEASE_SIGNING.md). Andere openstaande acceptatieproeven blijven voorwaarden voor publicatie.

De desktop gebruikt Yarn 1.22.22 met één `yarn.lock`; een schone installatie is `cd desktop-app && corepack yarn install --frozen-lockfile`. Pull requests en pushes naar `main` draaien de desktopverificatie. Een release start uitsluitend via `workflow_dispatch` op `main` met de volledige, gepushte 40-tekens commit-SHA uit de afzonderlijke `Fill84/ha-integration`-repository. `package.json`, `Cargo.toml` en `tauri.conf.json` moeten dezelfde desktopversie bevatten; de release bevat een manifest met beide broncommits en SHA-256-hashes van alle artifacts. De daadwerkelijke platform-, installer- en HA-compatibiliteitsproeven zijn nog releasevoorwaarden. Zie de [uitvoeringsstatus](docs/plans/2026-09-24-verificatiestatus.md).

De zelfgehoste Forgejo-runner met label `windows-latest` moet vóór de workflow de [Microsoft C++ Build Tools met de workload Desktop development with C++](https://v2.tauri.app/start/prerequisites/#microsoft-c-build-tools) en een Windows SDK hebben. De workflow gebruikt een al geconfigureerde `cl.exe`/`link.exe`-omgeving of laadt die via Visual Studio `vswhere.exe` en `vcvars64.bat`. Ontbreken zowel de bouwomgeving als `vswhere.exe`, dan moet de runnerbeheerder de workload op de **runnerhost** installeren en de runnerdienst opnieuw starten; het aanpassen van de app of de Rust-toolchain kan de ontbrekende Windows-linker niet vervangen. De Forgejo-verificatie pusht geen publieke release.

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
- Windows: NSIS installer (.exe)
- macOS: DMG (.dmg)
- Linux: DEB (.deb), RPM (.rpm), AppImage

## CPU-temperatuur en hostbelasting

De app bevat geen WinRing0-driver. Op Windows installeert onze NSIS-installer standaard de meegeleverde, Microsoft-ondertekende PawnIO-driver en onze eigen Rust-sensorservice; de gewone app vraagt alleen een beperkte lokale momentopname aan. Er is geen .NET-helper of aparte sensorapp. Zonder ondersteunde meting blijft CPU-temperatuur `unknown`. De actuele lokale 1.0.5-installer is op `phill-pc` en `beast-unit` als upgrade beproefd en leverde op beide hosts verse CPU-metingen via de bestaande HA-entities. Langdurige storing-, clean-install- en platformproeven blijven releasevoorwaarden.

De collector levert één snapshot per meetronde en hergebruikt de basis-systeemmetingen. Zie de [geïntegreerde sensorspecificatie](docs/specs/2026-09-24-integrated-sensors.md), het [uitvoeringsplan](docs/plans/2026-09-24-integrated-sensors.md) en de [sensor-supportmatrix](docs/specs/2026-09-24-sensor-support.md). Bestaande WinRing0-services worden niet automatisch verwijderd; een upgrade van een oude installatie vraagt nog migratievalidatie.

## License
Zie LICENSE.md voor licentievoorwaarden.

## Contact & Support
Voor vragen, issues of feature requests: open een issue op GitHub.

---

**Made with ❤️ for Home Assistant users!**
