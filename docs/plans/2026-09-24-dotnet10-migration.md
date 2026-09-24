# Migratie van de optionele sensorhelper naar .NET 10

Datum: 24 september 2026. Status: lokaal geïmplementeerd en geverifieerd; geen release gepubliceerd.

## Wijzigingen

- Helper target: `net10.0-windows`; de Tauri/Rust-basisapp blijft onafhankelijk van .NET.
- SDK: `10.0.401`, centraal in root-`global.json`, met `latestPatch` en zonder previews. De Windows-releaseworkflow gebruikt hetzelfde bestand via `actions/setup-dotnet`.
- Het buildscript voert dotnet vanuit de helperdirectory uit, zodat SDK-selectie ook bij een aanroep buiten de repository de juiste `global.json` vindt.
- NuGet-lockfile opnieuw gegenereerd voor .NET 10, Windows x64 en ARM64. LibreHardwareMonitorLib blijft expliciet op 0.9.6; de bestaande protocol-/providercontrole blijft geldig. Dependency-/licentie-inventaris bijgewerkt voor de geselecteerde .NET 10-assets.
- Nieuwe uitvoermap `desktop-app/src-tauri/resources/hwmon-net10`; de bundler plaatst uitsluitend deze inhoud onder `hwmon/`. De debug-resolutie gebruikt dezelfde nieuwe bronmap. Eventuele oude .NET 8-buildbestanden worden niet meegebundeld.
- Interface, README en providerdocumentatie vermelden .NET 10. Alleen gebruikers van de optionele helper hebben de passende .NET 10 Runtime nodig; die wordt niet automatisch geïnstalleerd of meegebundeld.

## Verificatie

Voor de lokale controles is de officiële Windows x64 SDK-ZIP in de genegeerde map `desktop-app/src-tauri/target/dotnet10-sdk` uitgepakt, na vergelijking van de SHA-512 met Microsoft-releasegegevens. SDK 10.0.401 bevat runtime 10.0.12. Er is geen globale SDK-/PATH-instelling gewijzigd; reguliere ontwikkeling vereist de SDK uit `global.json` op PATH.

| Controle | Resultaat |
|---|---|
| .NET 10 restore + Release build | Geslaagd, 0 warnings/errors |
| `node build-desktop.js` met .NET 10 op PATH | Frontend en Windows x64-helper gepubliceerd, locked restore |
| Publicatie Windows ARM64, frameworkafhankelijk, locked restore | Geslaagd; niet uitgevoerd op ARM64-hardware |
| Runtimeconfig van beide publicaties | `net10.0`, Microsoft.NETCore.App 10.0.0 als minimum; lokale uitvoering op 10.0.12 |
| Echte x64-helper, twee verzoeken zonder geïnstalleerde PawnIO-driver | Twee geldige `driver_missing`-antwoorden, juiste request-IDs, exit 0 |
| Rust-suite inclusief providercontract-/timeouttests | 22 geslaagd |
| Frontendtests voor providerinstelling | 2 geslaagd |
| NuGet vulnerability scan inclusief transitieve packages | Geen gemelde kwetsbaarheden op dit scanmoment |
| Publicatie-/bundlerinspectie | Alleen nieuwe providermap geselecteerd; geen .sys-bestanden |
| JS-syntax en diffcontrole | Geslaagd |

Het succesvolle ontbrekende-driverpad bewijst geen correcte fysieke temperatuurmetingen. De eerder vastgelegde Intel/AMD-/rechtenmatrix, performancebenchmarks en installer-/upgrademigratie blijven open; deze frameworkupgrade sluit die punten niet af.

## Bronnen

- [Microsoft-supportbeleid](https://dotnet.microsoft.com/en-us/platform/support/policy/dotnet-core): .NET 10 LTS ondersteund tot 14 november 2028; patches actueel houden.
- [Microsoft .NET 10-releasegegevens](https://builds.dotnet.microsoft.com/dotnet/release-metadata/10.0/releases.json): gebruikte SDK-/runtimeversie en archivechecksum.
- [SDK- en runtimeselectie](https://learn.microsoft.com/en-us/dotnet/core/versions/selection).
