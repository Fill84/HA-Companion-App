# Uitvoeringsplan bij de projectherstel-specificatie

Status: in uitvoering. Leidraad: [specificatie](../specs/2026-09-24-project-recovery.md). Elke fase eindigt met code-review, gerichte regressietests en bijgewerkt bevindingenregister. Onafhankelijke userwijzigingen in de geneste HA-repo blijven behouden.

1. **Beveiligingsgrenzen** (SEC-01–10): token alleen op vertrouwde HA-origin, TLS-validatie, WinRing0-removal/upgradepad, geheimen in logs/opslag, CSP, devserver, registratie-eigendom. Eerst SEC-01 en SEC-02 vanwege impact.
2. **Protocol en herstel** (REL-01–04, HA-01–04, ARCH-01): versieerde ACK, strikte payload, expliciete statuscodeclassificatie, heartbeat/interval en gezamenlijke beschikbaarheid. Test alle fout- en herstartsituaties vóór UI-wijzigingen.
3. **Lifecycle en instellingen** (REL-05–08, HA-05–07): locks, Windows-shutdown, autostart, persistente transacties, unload/repair en metadata.
4. **Sensormodel** (SEN-01–06, PERF-01–02): providerinventaris, stabiele identiteit, juiste eenheden, HA-semantiek, discovery en prestatiemeting. Optionele Windows-provider blijft uitgeschakeld als standaard en vereist geen meegeleverde WinRing0.
5. **UI en onderhoud** (UI-01–07, OBS-01, DOC-01–02): correct herstelgedrag, invoervalidatie, toegankelijkheid, logbeleid en actuele documentatie.
6. **Leveringsketen** (DEP-01–02, BUILD-01–07, TEST-01): lockfile, dependency-update, CI, versieconsistentie, signing en platform-/hardwarematrix. Alleen doorgaan naar release na de poort in de specificatie.

De fasen zijn afhankelijkheden, geen bewijs van voltooiing. Dit plan vereist na elke fase een hercontrole op regressies in de andere fase. Onderzoek naar een universele sensorbibliotheek wordt vastgelegd in een aparte supportmatrix; waar API's of hardware geen data bieden, maakt de UI die beperking zichtbaar.
