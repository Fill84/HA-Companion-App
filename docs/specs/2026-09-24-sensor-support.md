# Sensorbronnen en platformcontract

> De oorspronkelijke Windows-providerkeuze is vervangen door de [geïntegreerde sensoreis](2026-09-24-integrated-sensors.md). De bronnenvergelijking hieronder is historisch; de verificatiematrix noemt de actuele code en open proeven.

Onderzocht op 24 september 2026 voor de [projectherstelspecificatie](2026-09-24-project-recovery.md). **Conclusie:** er is geen aangetoonde, lichte, universele API die *alle* fysieke sensoren op *alle* hardware en Windows, macOS en Linux als één betrouwbare call ontsluit. Een call naar onze eigen collector kan wel één snapshot retourneren, maar die collector moet intern OS- en vendorproviders gebruiken. Een enkele IPC-call vermindert communicatieoverhead; hij maakt sensor-I/O niet gratis. Dit is een gevolgtrekking uit de uiteenlopende bron-API's hieronder, geen claim dat ieder toekomstig product is uitgesloten.

| Bron | Platform en bereik | Grenzen en keuze |
|---|---|---|
| [sysinfo](https://github.com/GuillaumeGomez/sysinfo) | OS-brede CPU, geheugen, schijven, netwerk, processen; componenttemperaturen waar het OS ze publiceert | Hergebruik handles en ververs gericht. Componentlabels bewijzen op zichzelf geen CPU-package-meting. Basisprovider. |
| [PawnIO](https://github.com/namazso/PawnIO) met eigen Rust-service | Windows: gecontroleerde, optionele kerneltoegang; momenteel alleen Intel CPU-package via de officiële IntelMSR-module | De app bouwt en bundelt de eigen service en de gesigneerde driver. Op `beast-unit` zijn installatie op een bestaand gestaged pakket en een echte 48–51 °C-meting via de eigen service bevestigd. Op `phill-pc` slaagden de schone driverinstallatie en een update van de bestaande HA-entity; de gebruiker zag daarna 52 °C in de kaart. De eerder onderzochte LibreHardwareMonitorLib/.NET-helper is uit de actuele build verwijderd. Een driver alleen levert niet automatisch alle hardwaremetingen. |
| [Linux hwmon/sysfs](https://docs.kernel.org/hwmon/sysfs-interface.html) | Linux: temperaturen, ventilatoren, spanning enz. mits een kernel-driver ze publiceert | Ontdek `/sys/class/hwmon` en identificeer chip/channel; labels, schaal en stabiliteit volgen kernel-ABI. Geen garantie voor elk apparaat. |
| [NVIDIA NVML](https://developer.nvidia.com/management-library-nvml) | NVIDIA GPU: thermiek, belasting, geheugen, ventilator waar ondersteund | Vendor-API; ontbrekende features zijn normaal. Bestaande projectprovider behouden en benchmarken. |
| [Apple device sensor-overzicht](https://developer.apple.com/documentation/technologyoverviews/device-sensors) en [MetricKit](https://developer.apple.com/documentation/MetricKit) | macOS: uiteenlopende, deels geautoriseerde API's; MetricKit levert vooral appdiagnostiek over een tijdvenster | Niet gelijk aan een publieke universele CPU-/moederbordtelemetrie-API. Native OS-metingen alleen tonen als herkomst en betekenis zijn gevalideerd. |

## Verplicht providercontract

Elke reading krijgt `stable_source_id`, apparaat-ID, meettype, ruwe numerieke waarde, eenheid, tijdstip, providernaam/-versie, beschikbaarheidsstatus en optioneel foutcode. De collector voert ontdekking apart van periodieke sampling uit, bewaart providerhandles, begrenst individuele calls en retourneert één snapshot naar de app. Normalisatie naar HA gebeurt centraal; `null` of `unavailable` betekent geen echte meting. Dynamic sensors mogen alleen ontstaan uit aantoonbaar herkende apparaten en bekende eenheden. Een provider mag geen identiteiten baseren op volgorde of aantallen.

Dit is het beoogde volledige contract; de huidige implementatie voldoet er nog niet overal aan. Voor GPU's, batterijen en de Windows-videocontroller is een bewaarde koppeling op NVML-UUID, PNP-ID of batterijserienummer toegevoegd. Bij onzekere meervoudige koppeling worden oude index-ID's niet hergebruikt; de oude HA-waarde wordt bij een volledige snapshot `unknown`. Een enkele bron zonder stabiel apparaat-ID gebruikt voorlopig zijn bestaande enkelvoudige ID. Netwerk en schijven vragen nog een gelijkwaardige fysieke identiteit en migratieproef.

Het contract voor deze release is **read-only monitoring**. Het wijzigen van fan curves, voltage of andere hardware-instellingen is geen impliciete bevoegdheid van `sensors beheren`. Dat vereist een afzonderlijke veiligheidsspecificatie, hardware-allowlist en expliciete gebruikerstoestemming.

## Verificatiematrix

| Host | Basisstatistiek | CPU-thermiek | GPU | Moederbord/fan | Status |
|---|---|---|---|---|---|
| Windows x64/arm64 | `sysinfo` | eigen optionele PawnIO-service voor Intel x64; AMD/ARM64 nog `unknown` | NVML of WMI | geen generieke moederbord-/fanprovider | x64 NSIS gebouwd met gesigneerde driver en eigen service; op `beast-unit` is een echte meting via de service bevestigd. Op `phill-pc` is de volledige route tot in Home Assistant bevestigd. |
| Linux x64/arm64 | `sysinfo` | herkende hwmon-componenten indien driver/label valide | NVML of beperkte OS-bronnen | geen volledige moederbord-/fanprovider | `render-unit` publiceert via `coretemp` een `Package id 0`-kanaal; app-build en runtimeproef ontbreken. |
| macOS Intel/Apple Silicon | `sysinfo`/native | uitsluitend gevalideerde OS-component | native/vendor waar beschikbaar | geen algemene garantie | Build en fysieke proef open. |

Per rij zijn nog nodig: koude en warme sampletijden, CPU-verbruik, geheugengebruik, gedrag bij verwijderde hardware, ontbrekende rechten, slaap/herstart en langdurige stabiliteit. Tot die metingen is er geen onderbouwde impactclaim of universele dekkingsclaim.
