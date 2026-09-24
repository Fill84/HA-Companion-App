# Optionele CPU-temperatuurprovider

De basisapp gebruikt geen eigen kernel-driver. De Windows-helper levert CPU-temperatuur alleen wanneer zowel de provider als de temperatuursensor is ingeschakeld. CPU-gebruik, frequentie en model blijven onafhankelijk beschikbaar.

## Vereisten en gebruik

- Windows x64 is lokaal gebouwd/getest. Windows ARM64 heeft een buildpad maar is nog niet op hardware gevalideerd.
- De helper wordt met .NET 10 als self-contained single-file executable gebundeld; er is geen afzonderlijke .NET-runtime-installatie nodig. Een nieuwe appbuild moet de meegeleverde runtime periodiek bijwerken met beveiligingsupdates.
- Voor temperatuur: de officiële, ondertekende [PawnIO](https://pawnio.eu/)-distributie, versie 2.2 of nieuwer, afzonderlijk geïnstalleerd. De unrestricted/unsigned variant is niet de ondersteunde route. De app installeert of beheert deze driver niet.
- Schakel in Instellingen de optionele CPU-temperatuurprovider in en sla op. Laat ook CPU Temperature aanstaan. Geen geldige meting betekent `unknown`; overige sensoren blijven werken.

Hardwareondersteuning en toegangsrechten blijven bepalend. De helper verhoogt zijn rechten niet; als de driver of hardware onder de huidige gebruiker niet toegankelijk is, moet de meting unknown blijven. De werkende temperatuurroute vereist nog praktijktests met ondersteunde Intel/AMD-hardware en een normale gebruikerssessie.

Installeer/vervang een provider of driver terwijl de optie uitstaat; schakel daarna opnieuw in. De helper hergebruikt zijn hardwarediscovery tijdens de sessie. Versie- en protocolvalidatie controleert compatibiliteit, maar is geen cryptografisch bewijs van herkomst van een lokaal vervangen executable. Gebruik de meegeleverde bestanden uit de gecontroleerde appbuild.

## Architectuur en belasting

Rust levert één sensorsnapshot per meetronde. De optionele helper initialiseert LibreHardwareMonitorLib **0.9.6** één keer en leest CPU-hardware op verzoek via stdin/stdout. Hij heeft geen polltimer, HTTP-server, shellinterface, HA-token of toegang tot de HA-configuratie. Het starten gebeurt zonder zichtbaar consolevenster en zonder elevation.

De library vernieuwt intern haar CPU-meetgroep; één JSON-call betekent niet één hardware-instructie. De helper voegt een .NET-proces toe zolang de optie actief is. Alleen echte metingen op ondersteunde hardware kunnen het totale CPU-/geheugenverbruik aantonen. Self-contained publicatie maakt de installer groter, maar voorkomt een extra runtime-installatie. Meet de procesbelasting op beide Windows-testhosts voordat de release wordt vrijgegeven.

De basiscollector doet gerichte CPU-/RAM-/procesrefreshes en hergebruikt de System-instantie. Andere bestaande providers, zoals GPU/WMI/disks/netwerk, zijn nog niet allemaal omgebouwd naar één gedeelde cache. De bredere performancebevindingen uit de audit blijven deels open.

## Meetcontract

Verzoek (één JSON-regel):

```json
{"protocol":1,"request_id":1}
```

Antwoord:

```json
{"protocol":1,"request_id":1,"provider":"librehardwaremonitor","provider_version":"0.9.6","status":"ok","readings":[{"hardware_id":"/intelcpu/0","label":"CPU Package","celsius":51.5}]}
```

Rust verifieert protocol, request-ID, provider en libraryversie; begrenst het antwoord op 32 KiB en 256 metingen; en accepteert alleen Intel/AMD CPU-identiteiten met erkende package/die-labels. Per CPU heeft `CPU Package`/`Core (Tdie)` voorrang op `Core (Tctl/Tdie)`. Willekeurige coregemiddelden, CCD-waarden, GPU's en ACPI-thermal-zones zijn geen CPU-packagefallback. Ontbrekende of ongeldige waarden worden niet gebruikt.

Bij meerdere CPU's rapporteert de bestaande entity `cpu_temperature` het maximum van erkende metingen. `measurement_source`, `sensor_labels`, `aggregation` en `provider_status` verduidelijken de herkomst. De HA-ID blijft behouden. Mislukte metingen verzenden null om oude waarden te vervangen.

Geen antwoord binnen drie seconden verbreekt de sessie; start-/protocolfouten leiden tot 60 seconden herstelpauze. De OS-aanroepen voor processtart/-beëindiging hebben geen harde realtimegarantie. Ontbrekende driver/hardware wordt als geldig statusantwoord behandeld. Provider of temperatuursensor uitschakelen beëindigt het helperproces. De volgende meetronde verstuurt bij een uitgeschakelde provider null; het afzonderlijke bestaande gedrag bij het volledig uitschakelen van een sensor staat nog in auditbevinding SEN-04.

Linux/macOS hebben geen meegeleverde helper. Alleen expliciet herkende native CPU-package/die-componenten worden gebruikt. Andere temperatuurbronnen blijven unknown; de platform-/hardwarematrix moet nog worden gevalideerd.

## Bouwen en testen

Vanaf `desktop-app` met een .NET 10 SDK:

```powershell
node build-hwmon.js
node --test tests/temperature-settings.test.cjs
cargo test --locked --lib --manifest-path src-tauri/Cargo.toml
```

De SDK-keuze staat centraal in `global.json`: 10.0.401, met uitsluitend patch-roll-forward en zonder previews. De Windows-CI leest hetzelfde bestand. Installeer deze SDK voor ontwikkeling; eindgebruikers hoeven geen afzonderlijke .NET-runtime te installeren.

De Windows release-prebuild bouwt de helper automatisch. Voor `tauri dev` eerst de helperbuild uitvoeren wanneer je deze optie wilt gebruiken. Cargo-tests vereisen geen driver of gebouwde helper. NuGet-versies/hashes liggen vast in `packages.lock.json`; een libraryupgrade vereist ook expliciete contract-/selectietests en review van de driverroute.

## Migratie van eerdere versies

Deze bronversie bevat geen WinRing0-module, driverbinary, runtime-extractie of NSIS-driverhook meer. Indirecte OHM/LHM-WMI- en generieke ACPI-fallbacks zijn verwijderd. De audit en oude plannen blijven historische bronnen en mogen niet opnieuw als implementatie-instructie worden gebruikt.

Dit verwijdert **niet automatisch een eerder geïnstalleerde systeemservice**. Eerdere versies bewaarden geen betrouwbaar eigenaarschap van `WinRing0_1_2_0`. Een service met die naam of een gelijknamig bestand in System32 kan ook bij andere software horen. Een reeds geïnstalleerde oude NSIS-uninstaller bevat bovendien nog zijn oude verwijderhook; een upgrade kan die aanroepen. Upgrade en veilige migratie moeten daarom vóór publicatie in een VM worden gevalideerd. Alleen de nieuwe installerconfig aanpassen lost dit bestaande-installatieprobleem niet op.

## Herkomst

Zie [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md), [MPL-2.0](LICENSE.txt) en de meegeleverde licentiebestanden. Bronlibrary: [LibreHardwareMonitor v0.9.6](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/tree/v0.9.6). Deze versie gebruikt [PawnIO-moduletoegang](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/blob/v0.9.6/LibreHardwareMonitorLib/PawnIo/PawnIo.cs); de helper implementeert geen eigen MSR- of servicebeheer.
