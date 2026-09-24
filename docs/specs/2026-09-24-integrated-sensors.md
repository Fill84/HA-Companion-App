# Geïntegreerde sensortoegang — herziene eis

Status: vastgesteld gebruikersdoel, implementatie en systeemproeven open. Deze specificatie vervangt de optionele extern geïnstalleerde Windows-provider uit `2026-09-24-sensor-support.md` voor de beoogde release. De audit en bestaande verificatiematrix blijven van toepassing.

## Resultaat

De geïnstalleerde Home Assistant Companion ontdekt, leest, laat kiezen en verstuurt sensoren zonder aparte sensorapp, apart te installeren provider of .NET-helper. Windows gebruikt de officieel ondertekende PawnIO-driver en daarvoor geschikte ondertekende modules als *meegeleverde onderdelen van onze installatie*. De applicatie implementeert zelf de drivercommunicatie, apparaatselectie, omzetting van ruwe meetwaarden, foutafhandeling en Home Assistant-normalisatie. LibreHardwareMonitorLib en de huidige `ha-hwmon.exe` verdwijnen uit de release.

Linux leest via de aanwezige kernelinterfaces, waaronder `hwmon` en `sysfs`; macOS via ondersteunde OS-interfaces. Op die platforms wordt geen PawnIO of Windows-driver meegeleverd. Eén app-call geeft een sensorsnapshot terug, maar interne OS-/hardwarecalls blijven nodig. Er is geen onderbouwde garantie dat elk apparaat op elk platform iedere fysieke sensor publiceert. Niet-bewezen waarden blijven `unknown`.

## Windows-grens en procesmodel

PawnIO is zelf de kerneldriver. Windows moet een ondertekend driverpakket met beheerdersrechten installeren en laden; de `.sys` kan niet alleen als onbeladen bestand in de GUI functioneren. De officiële INF beperkt de apparaatinterface tot `SYSTEM` en `Administrators`. De gewone Tauri-GUI krijgt daarom geen brede toegang tot driver-IOCTL's en draait niet permanent verhoogd.

De installatie levert een eigen, minimaal bevoorrecht sensoronderdeel. Dat opent de driver, laadt uitsluitend gecontroleerde, officieel ondertekende leesmodules en biedt aan de GUI alleen een versieerd, alleen-lezen snapshotcontract. Er is geen willekeurige MSR-, I/O-, modulelaad- of schrijfopdracht via de GUI-IPC. De transportlaag verifieert provider-/protocolversie, module- en driverhashes, grenzen en time-outs. Een ontbrekende, geblokkeerde of incompatibele driver geeft een expliciete status en geen verzonnen temperatuur. Het sensoronderdeel moet aantoonbaar de Windows-DACL respecteren zonder de PawnIO-device-interface voor alle gebruikers open te zetten.

Driverinstallatie, upgrade en verwijdering zijn onderdeel van onze installer en worden apart op schone en bestaande Windows-systemen beproefd. Alleen door onze installatie beheerde onderdelen mogen bij uninstall verwijderd worden; een gedeelde PawnIO-installatie van andere software mag niet worden verwijderd. Een eventueel UAC-verzoek is een OS-vereiste en wordt vooraf in de installer uitgelegd. Er wordt geen ongetekende of `unrestricted` driver op een releasehost geladen.

## Metingen en compatibiliteit

De eerste directe Windows-temperatuurroute leest alleen gevalideerde CPU-package-/diebronnen. Voor Intel zijn `IA32_TEMPERATURE_TARGET` en `IA32_PACKAGE_THERM_STATUS` kandidaten; de bits, CPU-familie, statusvlag, TjMax en thread-/package-affiniteit worden tegen de fabrikant- en modulecontracten gevalideerd vóór een °C-waarde naar HA gaat. AMD/ARM en andere bronnen krijgen afzonderlijke adapters en hardwareproeven. Het aanwezig zijn van PawnIO bewijst geen sensorondersteuning. Apparaat-/sensoridentiteiten mogen niet alleen op lijstvolgorde berusten.

De bestaande HA-unique-ID `cpu_temperature` en andere reeds toegewezen entity-ID's blijven behouden. Onzekere bronkoppelingen leveren `unknown`, overeenkomstig de bevestigde migratieregel. De kaart die op een HA-waarde `unknown` momenteel `NaN` toont, is een afzonderlijke dashboardpresentatiekwestie; de app mag daarvoor geen meting fabriceren.

## Acceptatie

1. Geen LibreHardwareMonitorLib, .NET-runtime of `ha-hwmon.exe` in de desktoprelease en geen aparte sensorapp of handmatige driverdownload voor eindgebruikers.
2. Gecontroleerde officiële driver-/module-artefacten met vastgelegde versie, SHA-256, handtekening en licentie/broncodeaanbod; een reproduceerbare installerbuild bevat ze.
3. Schone install, update, reboot, normaal gebruikersaccount, ontbrekende rechten, driverfout, slaap/herstel en uninstall getest op twee Windows-computers. Geen ongewenste driververwijdering.
4. Een echte CPU-packagewaarde op ondersteunde Intel- en AMD-hosts met onafhankelijke referentiemeting; niet-ondersteunde hardware blijft `unknown`.
5. Linux- en macOS-implementatie, buildconfiguratie en geautomatiseerde controles blijven vereist; per type worden dekking en ontbrekende OS-API's vastgelegd, zonder universele dekkingsclaim. De echte sensorproeven op die twee OS'en zijn op verzoek uitgesteld en blijven zichtbaar als open verificatiepunt.
6. CPU-/geheugen-/I/O-impact gemeten; behoud van HA-ID's en automatiseringen bij migratie en wegvallende bronnen getest.

## Gecontroleerde bronnen

- [PawnIO-broncode en GPL-2.0-licentie](https://github.com/namazso/PawnIO); de INF bevat de standaard-DACL en de officiële library documenteert de IOCTL-interface.
- [PawnIO.Setup 2.2.0](https://github.com/namazso/PawnIO.Setup/releases/tag/2.2.0) en [PawnIO.Modules](https://github.com/namazso/PawnIO.Modules); releaseartefacten worden bij opname opnieuw geverifieerd.
- [Windows-regels voor driverondertekening](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/windows-driver-signing-tutorial).
- [Linux hwmon-ABI](https://docs.kernel.org/hwmon/sysfs-interface.html).
