# CPU-metingen zonder verplichte extra driver

Status: ontwerp voor de door de gebruiker gekozen richting, 23 september 2026. De audit is goedgekeurd als onderzoeksbasis; deze wijziging behandelt alleen WinRing0 en CPU-temperatuur, niet alle auditbevindingen.

De lokale implementatie en de toen open praktijkvalidatie zijn vastgelegd in [het historische verificatieverslag](2026-09-23-temperature-provider-verification.md). De actuele richting staat in de [geïntegreerde sensorspecificatie](../specs/2026-09-24-integrated-sensors.md).

Op verzoek van de gebruiker is de optionele helper daarna naar .NET 10 LTS gemigreerd; zie [migratie en verificatie](2026-09-24-dotnet10-migration.md).

## Besluit en alternatieven

De basisapp gebruikt geen zelf meegeleverde kernel-driver. CPU-gebruik, frequentie en model blijven via sysinfo beschikbaar. Windows-CPU-temperatuur is optioneel, standaard uitgeschakeld en krijgt een afzonderlijk helperproces met een vastgepinde LibreHardwareMonitorLib-versie. Alleen wanneer de gebruiker daarvoor kiest en de ondersteunde PawnIO-installatie aanwezig is, probeert de helper een meting. De app installeert, start, stopt of verwijdert zelf geen driverservices.

Alternatieven: uitsluitend OS-metingen is eenvoudiger maar verliest Windows-CPU-temperatuur; een verplichte vervangende driver behoudt meer temperatuurmogelijkheden maar past niet bij de gekozen basis zonder verplichte driver. Een willekeurige bestaande LHM/OHM-WMI-provider is onvoldoende gecontroleerd: daarmee kunnen oudere driverafhankelijkheden ongemerkt terugkomen.

LibreHardwareMonitor 0.9.6 gebruikt PawnIO voor de betreffende registertoegang. Dat is nog steeds een kernel-driver, geen garantie van risicoloosheid. Windows' Win32_TemperatureProbe vult CurrentReading niet. Generieke ACPI-thermal-zones mogen niet als CPU-package worden gepresenteerd.

## Eisen

- TMP-01: verwijder WinRing0-bronmodule, driverbinary, bundleresource, NSIS-hooks en uitsluitend daarvoor benodigde directe Windows-API-features.
- TMP-02: geen impliciete OHM/oude-LHM-, ACPI- of installatiefallback. Een ontbrekende meting wordt null/unknown met een statusreden; nooit 0 of de vorige waarde.
- TMP-03: Windows-provider is een expliciete, opgeslagen opt-in; oude configuraties krijgen standaard uit. Save/Cancel geldt ook voor deze keuze.
- TMP-04: helper gebruikt een vastgepinde library, een versieerbaar JSON-contract, alleen CPU-hardware en een begrensde uitvoering/output. Geen HA-credentials, shellcommando of gebruikersgekozen executablepad.
- TMP-05: alleen erkende package/die-sensoren; bron, sensorlabel en aggregatie zichtbaar als HA-attributes. Bij meerdere CPU-packages: maximum van de erkende package/die-metingen. Geen gemiddelde van willekeurige cores/CCD's als package.
- TMP-06: temperatuur uitgeschakeld betekent geen temperatuurprobe; CPU-model/usage-only doet geen privileged temperatuurwerk.
- TMP-07: bestaande HA-ID cpu_temperature blijft behouden; onbeschikbaarheid overschrijft oude waarden met null.
- TMP-08: Linux/macOS gebruiken uitsluitend herkenbare native CPU-componenten, zonder extra geïnstalleerde driver vanuit deze app; unsupported blijft unknown. Deze platforms vragen afzonderlijke runtimevalidatie.
- TMP-09: nieuw installeren/uninstalleren beheert geen gedeelde drivers. Historische audit/plannen blijven als historie herkenbaar.

## Aanvulling: één snapshot en lage hostbelasting

De gebruiker vraagt om zoveel mogelijk sensorgegevens met één kleine call en weinig hostbelasting. Het publieke collectorcontract blijft daarom één snapshot per meetronde. Dat is geen belofte dat alle hardware via één OS-call beschikbaar is: verschillende providers blijven nodig voor OS-, GPU- en CPU-temperatuurgegevens.

De optionele helper blijft gedurende gebruik actief, ontdekt hardware één keer en meet op verzoek. Geen nieuw proces of volledige hardwarediscovery per sensor/cyclus. Statische gegevens worden gecachet, dynamische providers gedeeld, uitgeschakelde meetgroepen niet uitgelezen. Een mislukte snapshot mag geen onbegrensde retrylus of oude waarde opleveren.

HWiNFO shared memory is een alternatief wanneer die software al aanwezig is. Het extra uitlezen kan beperkt blijven, maar de onderliggende collector blijft resources gebruiken en de gratis SHM-interface is tot 12 uur begrensd. Daarom geen verplichte basisafhankelijkheid. Zie [officiële SHM-uitleg](https://www.hwinfo.com/forum/threads/important-changes-to-hwinfo64-coming-soon.7092/). Sysinfo ondersteunt [gerichte refreshes](https://docs.rs/sysinfo/latest/sysinfo/struct.System.html); LHM ondersteunt [hergebruik van een Computer en UpdateVisitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor).

Acceptatie moet totaalverbruik van app plus helper meten: CPU-tijd, private memory, processtarts en p50/p95-meetduur bij 10/60 seconden interval, basis versus uitgebreide provider, met vastgelegde hardware. Eerst een baseline; geen vooraf verzonnen claim zoals minder dan 1% CPU op iedere pc. Ondersteuning van alle denkbare sensoren op ieder apparaat is geen haalbare garantie.

## Migratiegrens

Oude versies registreerden WinRing0_1_2_0 zonder betrouwbare eigendomsregistratie. Dezelfde service kan door andere software worden gebruikt. Automatische verwijdering van alleen die servicenaam of van het bestand in Windows/System32/drivers is daarom niet verantwoord. Ook een oude uninstaller kan bij een upgrade zijn oude destructieve hook uitvoeren. Een nieuwe hook verwijderen kan die reeds geïnstalleerde uninstaller niet achteraf repareren. Upgrade van een bestaande installatie moet in een VM worden onderzocht vóór publicatie; de bronwijziging alleen claimt geen volledig opgeschoond gebruikerssysteem.

## Uitvoeringsvolgorde

1. Verwijder oude driverroute en scheid basismeting van temperatuur.
2. Bouw vastgepinde optionele helper, contractvalidatie en timeout-/foutafhandeling.
3. Verbind providerkeuze met settings, HA-attributes en Windows-bundling.
4. Test selectie, null/error/timeout, uitgeschakelde provider, versiecontrole, settings en packaging; documenteer installatie en migratiegrenzen.

## Bronnen

- [LibreHardwareMonitor 0.9.6](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/releases/tag/v0.9.6)
- [PawnIO-toegang in deze libraryversie](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/blob/v0.9.6/LibreHardwareMonitorLib/PawnIo/PawnIo.cs)
- [Microsoft: Win32_TemperatureProbe](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-temperatureprobe)
- [PawnIO-distributies en driverkarakter](https://pawnio.eu/)
