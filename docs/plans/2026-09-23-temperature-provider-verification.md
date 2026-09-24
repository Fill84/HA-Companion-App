# Verificatie: WinRing0 verwijderen en optionele temperatuurprovider

Datum: 23 september 2026. Lokale implementatie; geen commit, push, installeruitvoering of release. De oorspronkelijke audit blijft een snapshot van de oude bronversie.

Vervolg: de helper is op 24 september gemigreerd van .NET 8 naar .NET 10. De nieuwe framework-/buildresultaten staan in [het migratieverslag](2026-09-24-dotnet10-migration.md); onderstaande resultaten beschrijven de oorspronkelijke implementatie.

## Gewijzigd

- Verwijderd: WinRing0 Rust-module, driverbinary, module-export, gebundelde driverresource, NSIS-installatie-/verwijderhooks en de uitsluitend hiervoor gebruikte directe Windows-API-features.
- Verwijderd: CPU-fallbacks naar willekeurige LHM/OHM-WMI-providers en generieke ACPI/thermal-zonewaarden. Basismeting en temperatuur zijn gescheiden.
- Toegevoegd: uitgeschakelde-by-default Windows-providerkeuze, opgeslagen via settings en UI. Oudere instellingen schakelen hem niet in; setup-aanroepen zonder het nieuwe veld behouden de bestaande keuze.
- Toegevoegd: blijvend helperproces met LibreHardwareMonitorLib 0.9.6 en vastgelegde NuGet-resolutie. Geen eigen driverbeheer, elevation, HA-credentials of HTTP-server.
- Toegevoegd: request-ID-/versievalidatie, beperkte output, timeout, herstelpauze, expliciete null/status en package-/die-selectie. De bestaande HA-sensor-ID blijft gelijk.
- De basiscollector hergebruikt System en ververst gericht CPU, RAM en de noodzakelijke proceslijst. De dubbele volledige refresh en afzonderlijke dynamische process-enumeratie zijn verwijderd. Initieel geheugen blijft beschikbaar voor statische memory_total.
- Windows-bundling bouwt/kopieert de helper en notices; .NET-runtime en PawnIO worden niet gebundeld. Het oude fase-2-plan is als historisch gemarkeerd.

## Werkelijk uitgevoerde controles

| Controle | Uitkomst |
|---|---|
| `cargo test --locked --lib --manifest-path desktop-app/src-tauri/Cargo.toml` | **22 passed**, geen overgeslagen tests |
| `node --test tests/temperature-settings.test.cjs` vanuit desktop-app | **2 passed** |
| `dotnet build hwmon-helper/HwmonHelper.csproj --no-restore` | Geslaagd, 0 warnings/errors |
| `node build-desktop.js` vanuit desktop-app | Frontend + frameworkafhankelijke Windows x64-helper gepubliceerd met locked NuGet-restore |
| `cargo build --locked --manifest-path desktop-app/src-tauri/Cargo.toml` | Volledige debug-app gebouwd; niet gestart |
| Echte helper met twee verzoeken, na read-only controle dat PawnIO niet geïnstalleerd is | Twee geldige driver_missing-antwoorden, juiste request-IDs, lege readings, exit 0 |
| Nieuwe procescontracttests met fake providers | Zelfde PID voor twee snapshots; timeout bij hang; te grote output afgewezen |
| Nieuwe selectie-/migratie-/payloadtests | Legacy provider afgewezen, GPU/corefallbacks uitgesloten, oude settings opt-out, stabiele entity-ID + null |
| `dotnet list ... package --vulnerable --include-transitive` | Geen gemelde kwetsbare NuGet-packages op dit scanmoment |
| JS-syntaxchecks, JSON-configparse en documentlinks | Geslaagd |
| `git diff --check` | Geslaagd |
| Inspectie nieuwe debug-app en gepubliceerde helperbestanden | Originele driverimage niet aanwezig; geen .sys-bestand in helperoutput |
| Lokaal gepubliceerde helperbestanden | Circa 5 MB, exclusief apart geïnstalleerde runtime/driver |

Strict clippy wees naast bestaande auditproblemen aanvankelijk op het aantal named IPC-argumenten. Daarvoor is uitsluitend op save_settings een gemotiveerde uitzondering toegevoegd om de bestaande named setup-aanroepen te behouden. Een controle met `-D warnings` en alleen de vier bestaande lints uitgezonderd slaagt:

```powershell
cargo clippy --locked --all-targets --manifest-path desktop-app/src-tauri/Cargo.toml -- -D warnings -A clippy::manual_flatten -A clippy::let_unit_value -A clippy::missing_transmute_annotations -A clippy::manual_is_multiple_of
```

De gewone strikte lintgate is dus nog niet groen. De vier bestaande meldingen blijven onderdeel van de audit; deze gerichte controle is geen claim dat alle auditproblemen zijn opgelost.

## Nog niet bewezen / vervolgwerk

1. Werkelijke, correcte temperaturen op Intel/AMD met de officiële PawnIO-driver onder een normale gebruikerssessie; providervergelijking bij idle/load en meerdere CPU's.
2. Totale CPU-tijd, private memory en p50/p95-cycletijd van app plus helper. Geen percentage of algemene performancewinst geclaimd. GPU/WMI/disks/netwerk hebben nog bestaande optimalisatiepunten.
3. Schone NSIS/MSI-installatie, resourceplaatsing in echte installers en upgrades in wegwerp-VM's. Een oude geïnstalleerde uninstaller kan nog de oude gedeelde driverhook uitvoeren; geen automatische verwijdering zonder eigendomsbewijs.
4. Native UI/HA-end-to-end, macOS/Linux en Windows ARM64. De gewone HA-lifecycle-/registratieproblemen uit de audit zijn in deze wijziging niet opgelost.
5. Ondersteunde runtime-/driver-/hardwarematrix en ondertekening/provenance van definitieve distributie. De .NET-runtime is een afzonderlijke optionele prerequisite; langdurige distributie vraagt een actuele LTS-keuze.

SEC-03/04/05 en BUILD-03 hebben met deze bronwijziging geen nieuwe WinRing0-installatieroute meer. De migratie van oude installaties blijft open. SEN-01 is aangescherpt met bronselectie en aparte provider, maar vereist hardwarevalidatie. PERF-02 en TEST-01 zijn gedeeltelijk verbeterd en blijven breder open. Het volledige auditplan is niet uitgevoerd.

De al aanwezige wijziging in `ha-integration/.gitignore` en de auditbestanden zijn behouden. Genegeerde buildcaches kunnen oudere binaries bevatten; verspreid uitsluitend nieuwe, gecontroleerde release-artifacts. Deze werkzaamheden hebben geen Windows-driverservice geïnstalleerd, gestart, gestopt of verwijderd.
