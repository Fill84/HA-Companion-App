# Geleverde externe componenten

Bijgewerkt 24 september 2026. Deze inventaris beschrijft uitsluitend de handmatig meegeleverde of apart gepubliceerde onderdelen; de volledige Cargo- en Yarn-afhankelijkheden staan in hun lockfiles. Verifieer dit opnieuw bij iedere versie-update.

| Onderdeel | Vaste herkomst | Licentie/notices | Distributie |
|---|---|---|---|
| tsParticles Links Preset browserbundle | [`@tsparticles/preset-links` 3.2.0](https://www.npmjs.com/package/@tsparticles/preset-links/v/3.2.0), npm-tarballintegriteit `sha512-F4nddhEzMSiZZzIizXcb6jMbHEC7+hAwzrUph1OiRvHWnLeo13pDG0DdYIKhY15xC6cZP0IMZq7wzIUkyPJ1ew==`; SHA-256 van `tsparticles.preset.links.bundle.min.js`: `8b77405cac147e4eab3d943e55d3cd8d5910c916c9510dec8973553138d93025` | MIT: `desktop-app/vendor/LICENSE.tsparticles.txt` en door de bundelheader genoemd `tsparticles.preset.links.bundle.min.js.LICENSE.txt` | `build-web.js` kopieert bundel én beide notices naar de webassets. De hash is lokaal vergeleken met het bestand in de officiële npm-tarball. |
| LibreHardwareMonitorLib | NuGet 0.9.6, vast in `hwmon-helper/packages.lock.json` | MPL-2.0 en afhankelijkheidsnotices in `hwmon-helper/THIRD-PARTY-NOTICES.md`, `LICENSE.txt` en `licenses/` | Alleen optionele Windows-.NET 10-provider. `build-hwmon.js` kopieert de notices naar de helperresource. Geen WinRing0- of PawnIO-driver wordt meegeleverd. |

Updateprocedure: verhoog de versie in de bronconfiguratie, haal het officiële pakket op, controleer de pakketintegriteit en de hash van het werkelijk geleverde bestand, werk alle licentiebestanden en dit manifest bij, en test de distributie-artefacten. Een versienummer in geminificeerde code is onvoldoende herkomstbewijs; de tsParticles-identificatie hierboven is gebaseerd op een byte-identieke pakketvergelijking.
