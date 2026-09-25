# Dashboardrechtsklik en sensorinstellingen — 25 september 2026

## Bevindingen

- Het Home Assistant-dashboard draait in een afzonderlijke, externe Tauri-child-webview. De getoonde opties Back/Refresh/Save as/Print zijn het standaardcontextmenu van de webview. De lokale Settings-pagina draait in de andere webview. De gebruiker heeft na bespreking gekozen voor **geen rechtsklikmenu** in het dashboard.
- Settings bouwde zijn lijst uitsluitend uit `SENSOR_CHOICES`. Alle 23 groepen in die catalogus kwamen overeen met de 23 groepen die de collector controleerde. Individuele schijf-, GPU-, netwerk-, batterij- en schermmetingen waren echter niet afzonderlijk te kiezen. Ook voor enkelvoudige sensoren ontbrak een expliciete keuze per daadwerkelijke meting.
- De sensorlijst had een eigen scrolvlak van maximaal 240 px binnen de al scrollende Settings-dialoog. De metingen waren daardoor lastig te vinden en de lijst oogde onvolledig.
- Het HTML-veld voor het update-interval had minimum 10 seconden, terwijl frontend, backend en HA-protocol 5 seconden toestaan.
- De badge `Static (startup only)` was verouderd: de achtergrondlus verstuurt ook elke tien intervallen een volledige snapshot, en Opslaan synchroniseert sensorwijzigingen direct.
- Een volledige snapshot in de HA-integratie zet geregistreerde, niet langer doorgestuurde metingen op `unknown` zonder hun entity-ID te verwijderen. Dat is het bestaande contract waarop de individuele selectie aansluit.

## Overige Settings-velden

| Veld | Controle |
| --- | --- |
| Server-URL en token | De URL wordt door de backend gevalideerd. Een leeg tokenveld behoudt het opgeslagen token voor dezelfde server; het token zelf wordt niet teruggestuurd naar de UI. Bij een andere server is een passend token nodig. |
| Update-interval | De UI, backend en HA accepteren nu allemaal 5–3600 seconden. |
| Taal | Engels/Nederlands passen de vaste UI-teksten en groepsnamen aan. Namen die hardwareproviders ontdekken zijn nog niet vertaald. |
| Starten bij inloggen | Opslaan past de OS-autostartregistratie aan en draait die wijziging terug als het opslaan van instellingen faalt. |
| App- en integratieversie | De appversie komt uit de draaiende binary; de integratieversie wordt via een geauthenticeerde HA-aanvraag gelezen en toont een foutstatus als HA niet bereikbaar is. |
| Device-ID, webhook en status | De getoonde status `Registered` betekent dat lokaal een webhook-ID is opgeslagen; het is geen live-verbindingsmeting. `Reconnect` is een onmiddellijke registratieactie, los van de overige, nog niet opgeslagen velden. |
| Publiek IP | Wordt alleen op verzoek opgehaald. Een mislukte aanvraag toont een foutstatus. |

## Implementatie

- De dashboardwebview voorkomt het native rechtsklikmenu op de exacte geconfigureerde HA-origin. Er is geen app-IPC aan de externe pagina toegevoegd.
- Settings ontdekt nu de werkelijk aanwezige metingen bij openen. De 23 groepsschakelaars blijven bestaan; daaronder staan de afzonderlijke metingen. Een uitgeschakelde groep blokkeert zijn individuele keuzes, maar bewaart hun voorkeuren voor later. Individuele voorkeuren krijgen sleutels `sensor:<unique_id>` zodat bestaande groepsvoorkeuren, sensor-ID's en HA-entity-ID's intact blijven.
- De collector filtert uitgeschakelde individuele metingen uit zowel volledige als dynamische snapshots. De onmiddellijke synchronisatie na Opslaan maakt hun bestaande HA-entities `unknown`; opnieuw inschakelen gebruikt dezelfde ID.
- De sensorlijst gebruikt nu de ene scrollbalk van de dialoog. Groepen en metingen hebben verschillende badges en een korte uitleg. Het invoerveld accepteert nu 5–3600 seconden, conform de echte validatie.
- De badge van een minder vaak ververste meting vermeldt nu `Every 10 intervals` in plaats van `Static (startup only)`. De lokale registratiestatus heet `Registration saved`, omdat een opgeslagen webhook niet bewijst dat HA nu bereikbaar is.

## Grenzen en verificatie

- De lijst bevat de metingen die op deze computer op dat moment ontdekt worden. Een tijdelijk afwezig apparaat verschijnt weer zodra het opnieuw ontdekt wordt; zijn eerder opgeslagen voorkeur en ID blijven bewaard. Een groepsschakelaar blijft zichtbaar, ook als de groep nu geen metingen heeft.
- De sensornamen van dynamisch ontdekte hardware volgen momenteel de Engelse namen van de collector; de vaste groepsnamen en de Settings-bediening zijn in Engels en Nederlands beschikbaar.
- Een broncode- en testcontrole bevestigt de catalogusdekking, instellingensemantiek en HA-`unknown`-route. De gepubliceerde 1.0.5-installatie bevat deze wijzigingen niet.

## Lokale installatieproef op phill-pc — 26 september 2026

- Vanaf commit `4e0b377` is zonder versiebump een Windows NSIS-installer gebouwd (`Home Assistant Companion_1.0.5_x64-setup.exe`, SHA-256 `FF2786C8FF5DB23EBFB90894FF9CF0D76FF49CD5A84B8D537C7E87009E74A4EB`). Dit is een lokale proefbuild, geen nieuwe publieke 1.0.5-release.
- Voor installatie is `settings.json` bytegelijk geback-upt. De verhoogde, stille installatie eindigde met code 0. De instellingenshash was direct na installatie nog gelijk aan de back-up, en de sensordienst draaide. Het geïnstalleerde executable heeft dezelfde grootte als de bouwoutput en verschilt alleen in drie bytes die Tauri bij het bundelen aanpast.
- De geïnstalleerde GUI startte uit `C:\Program Files\Home Assistant Companion\ha-companion.exe`; de app-log bevestigt een HA-ping met HTTP 200 en een aangemaakte dashboardwebview. Een echte rechtsklik op een leeg dashboarddeel bracht geen Edge-contextmenu meer in beeld. De dashboardkaarten toonden actuele CPU-, RAM-, GPU- en CPU-temperatuurwaarden.
- De nieuwe Settings-lijst is op phill-pc nog niet interactief gecontroleerd: een poging via de Windows-tray verloor focus tijdens automatisering. De app bleef draaien en er zijn geen Settings-keuzes gewijzigd. Ook de per-meting-aan/uitroute is op deze geïnstalleerde build nog niet end-to-end tegen HA beproefd; de bestaande automatische tests dekken de logica.
