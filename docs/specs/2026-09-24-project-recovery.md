# Projectherstel: uitvoerbare specificatie

Status: werkdocument, 24 september 2026. Bron: [audit](../audit/2026-09-23/AUDIT.md) en [register](../audit/2026-09-23/findings.json). Deze specificatie omvat alle 54 bevindingen. Een eis is pas afgerond met een gerichte test of een vastgelegde, reproduceerbare controle. Bestaande, niet-gecommitte wijzigingen voor WinRing0 en .NET 10 worden opnieuw getoetst.

## Productcontract

De desktopapp mag zonder eigen kernel-driver starten. Niet-beschikbare hardwaremetingen worden `unknown` of ontbreken met een expliciete reden; er worden geen schattingen als fysieke sensormetingen gepresenteerd. Een optionele, gecontroleerde provider mag extra metingen leveren. Het HA-protocol moet onderscheid maken tussen HTTP-transport, een herkende opdracht en duurzaam verwerkte data. Het dashboard mag een HA-token uitsluitend aan de exacte, geconfigureerde HA-origin aanbieden. De app belooft geen universele sensordekking; dekking wordt per OS, vendor en provider aangetoond.

## Toetsbare eisen per auditbevinding

| IDs | Eis en acceptatiebewijs |
|---|---|
| SEC-01 | Bind tokeninjectie aan exacte scheme/host/port en top-level frame; blokkeer of externaliseer vreemde navigaties. Test redirect, frame, foutpagina en normale HA-login met neptoken. |
| SEC-02 | Verifieer TLS standaard; eigen CA alleen met expliciete trustconfiguratie. Test geldige en ongeldige certificaten. |
| SEC-03–05 | Geen WinRing0-binary, code, installer- of runtimepad. Ruim uitsluitend eigen legacy-installaties op na bewijs van eigendom. Test schone installatie en upgrade. |
| SEC-06–07 | Log nooit tokens/webhook-ID's of response bodies met geheimen; beperk tokenblootstelling via IPC en bescherm opslag volgens platformcontract. Test redactie en migratie. |
| SEC-08–10 | Restrictieve CSP en localhost-devserver. Bestaande HA-device-ID mag alleen door dezelfde geregistreerde gebruiker worden hergebruikt; toets andere gebruiker. |
| REL-01–04 | Elke webhookopdracht heeft een herkenbare versie en JSON-bevestiging. Alleen expliciete 404/410 maakt registratie ongeldig. Wijzigen van URL/token, gedeeltelijke registratie en herstel leveren één consistente toestand op; test netwerkuitval en herstart. |
| REL-05–08 | Shutdown heeft een eindige totale deadline; Windows-hook adresseert eigen HWND en respecteert annulering. Autostartschakelaar wijzigt werkelijke OS-registratie. Settings worden atomair of met expliciete opslagfout bewaard. |
| HA-01–04 | Eén bron voor online/offline; elk geldig heartbeat meldt interval, los van sensordata. Ongeldige payload wijzigt geen activiteit. Gewone entiteiten worden unavailable bij offline; test timer, shutdown en herverbinding. |
| HA-05–07 | Unload verwijdert timers, stores en handlers passend bij lifecycle; repair vereist geladen entry; metadata-updates worden doorgevoerd. |
| SEN-01–06 | Waarden hebben herkomst, eenheid en stabiele ID. Correcte GB/GiB, byte/s versus bytes, uptime en timestampsemantiek. Aan/uit en verdwenen hardware hebben expliciet entitybeleid. Test op ondersteunde platformen. |
| PERF-01–02 | Alleen gewijzigde metadata registreren; collectie op passende workers en met hergebruikte handles. Meet CPU, I/O en doorlooptijd op representatieve hosts. |
| OBS-01, ARCH-01 | Rotatie/begrenzing van logs op elk OS; één vastgelegd protocol en descriptorcontract met contracttests. |
| UI-01–07 | Escape werkt alleen in actieve modal; registratie is enkelvoudig en toont fouten; invoer is gevalideerd; annuleren herstelt conceptstaat; vertalingen, focus, schermhoogte en animatie volgen UI-contract. Test toetsenbord en mobiel venster. |
| DEP-01–02 | Werk direct gebruikte dependencies bij, documenteer resterende transitieve advisories en hun werkelijke toepasbaarheid; scan het gekozen lockfile in CI. |
| BUILD-01–07 | Eén JS-package-manager en schone install; geteste signing; installerupgrade; CI-gates; vastgelegde desktop-/helper-/HA-versies; OS-matrix; juiste minimum-HA-versie. |
| TEST-01 | Contract-, foutpad- en end-to-endtests voor desktop↔HA naast bestaande unit-tests. Hardware-afhankelijke tests gebruiken opt-in hosts. |
| DOC-01–02 | README's, supportmatrix, migratiegids en vendorlicenties weerspiegelen werkelijk product en distributie. |

## Releasepoort

Geen claim van volledige afronding zonder: alle 54 IDs met bewijs en status; schone Windows/macOS/Linux-builds voor geclaimde architecturen; geslaagde desktop-, HA- en protocoltests; upgrade- en uninstallerproef; echte HA-test; expliciet gedocumenteerde sensorbeschikbaarheid per platform. Hardware of OS dat niet beschikbaar is voor test blijft een open verificatiepunt, geen impliciete pass.
