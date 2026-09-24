# Verificatie bij projectaudit van 23 september 2026

Dit log beschrijft werkelijk uitgevoerde controles. Een geslaagde syntaxcheck is geen runtime- of integratietest. De volledige duiding en scope staan in [AUDIT.md](AUDIT.md).

## Uitkomsten

| Controle | Resultaat | Betekenis/beperking |
|---|---|---|
| `git status`, commits en inventaris beide repo's | Vastgelegd; 82 tracked bestanden | Integratie is afzonderlijke repo, genegeerd door hoofdrepo |
| Python AST-parse van getrackte `.py`-bestanden | Geslaagd | Geen HA-module-import of echte HA-runtime |
| JSON-parse getrackte JSON-bestanden | Geslaagd | Geen schema-/hassfest-validatie |
| `node --check` op main/settings/particles/i18n | Alle vier geslaagd | Alleen syntax |
| `node build-web.js` in desktop-app | Geslaagd: web assets copied | Bouwt alleen frontend-assets, geen installer |
| `cargo test --locked --lib -- --skip collect_returns_basic_cpu_data` | **13 passed; 0 failed; 1 filtered out** | Compileert Windows-lib/testbinary; de driveraanrakende hardwaretest overgeslagen |
| `python -m pytest -q -p no:cacheprovider ha-integration/tests` | **9 passed in 0.03s** | Pure availabilityhelpers; geen echte HA-lifecycle |
| `cargo fmt --all -- --check` | Exit 1, formatteringsverschillen | Geen formattering toegepast |
| `cargo clippy --locked --all-targets -- -D warnings` | Exit 1, vijf lintsoorten | Zie onderstaand overzicht |
| `npm ls --depth=0` | API 2.10.1, CLI 2.10.0, serve 14.2.5 | Geïnstalleerde lokale dependencies |
| `npm ci --dry-run --ignore-scripts` in desktop-app | Exit 1, package.json/lockfile niet synchroon | serve en transitieve dependencies ontbreken |
| `npm audit --json --package-lock-only` in desktop-app | 0 meldingen; locktree totaal 13 | Onvolledige tree; serve ontbreekt |
| `yarn audit --json` in desktop-app | 8 meldingen: 6 high, 2 moderate; tree totaal 100 | Devtoolafhankelijkheden; geen bewezen exploit in releasebinary |
| cargo-audit 0.22.2 op project-Cargo.lock | 11 vulnerability-matches + 14 waarschuwingen; 593 dependencies in lockfile | Alle lockfileplatforms/features; niet alles actief op Windows |
| Cargo reverse dependency tree voor rustls | Actief via reqwest | Daadwerkelijke standaard-Windows-tree |
| Cargo reverse dependency tree voor quinn-proto en quick-xml | Nothing to print | Geen match in gecontroleerde standaard-Windows-tree; geen algemene vrijwaring |
| `cargo tree --offline --target all -i nix@0.19.1` | Niet voltooid: niet-gecachete platformdependency | Geen volledig gecontroleerde cross-platform reachability |
| Driver SHA-256, versie, Authenticode | 1.2.0.5; 14.544 bytes; signature Valid | Geen bewijs van veiligheid; niet geladen/uitgevoerd |
| Signingproperty-reproductie met dummywaarde | Property assignment faalt | Geen certificaatimport/configwijziging/signing |
| Offline integratieprobe | Beschikbaarheidsfouten en inputexceptions gereproduceerd | Echte functies, minimale HA-stubs |
| Offline frontendprobe | Origininjectie en UI-fouten gereproduceerd | Echte JS/scriptstring, minimale DOM/IPC; geen native webview |
| Gerichte secretpatroonzoekactie in runtimebron en workflow | Geen echte hardcoded tokens/private keys aangetoond | Geen historie-/gebruikersconfig-/volledige entropy-scan |
| Hashvergelijking bronbestanden aan einde audit | Alleen `ha-integration/.gitignore` verschilt van inventarissnapshot | Deze tijdens de audit verschenen wijziging is niet door audittools aangebracht en bleef behouden |

De eerste ongespecificeerde `cargo test --locked --lib`-run is tijdens compilatie gestopt nadat code-inspectie liet zien dat de CPU-test driverbeheer kan aanroepen. De testbinary van die run is niet gestart. Daarna is de suite uitgevoerd met de expliciete skip hierboven. Er is geen kernel-driver of installer uitgevoerd.

## Clippybevindingen

Onder Rust 1.93.0 met `-D warnings`:

- `duplicated_attributes`: cfg(windows) in sensors/mod.rs én sensors/winring0.rs.
- `manual_flatten`: battery-iterator.
- `let_unit_value`: settings.rs:62, store.set retourneert unit.
- `missing_transmute_annotations`: shutdown_hook.rs:88.
- `manual_is_multiple_of`: lib.rs:287.

Deze bevindingen verklaren de mislukte lintgate; ze zijn niet ieder als zelfstandig functioneel defect geteld.

## Reproduceerbare aanvullende probes

Vanaf de projectroot:

```powershell
python -B docs/audit/2026-09-23/evidence/reproduce_integration.py
node docs/audit/2026-09-23/evidence/reproduce_frontend.cjs
pwsh -NoProfile -File docs/audit/2026-09-23/evidence/reproduce_signing.ps1
```

Er zijn geen echte tokens, HA-adressen, externe requests of OS-mutaties nodig. Alle resultaten gebruiken synthetische waarden. De eerste twee scripts verwachten bewust dat de gevonden defects nog reproduceerbaar zijn. Na herstel moet hun functie als bewijs worden vervangen door regressietests met de gewenste uitkomst.

De Pythonprobe bevestigt:

- expliciet offline → ten onrechte online bij de eerste timer-tick;
- offline-cache → korte heartbeat → blijvend online ondanks verlopen timeout;
- register_sensor HTTP 400 telt toch als heartbeat;
- null-envelope, niet-lege lijst als type, null-data en null-sensoritem veroorzaken exceptions;
- een niet-doorgegeven interval van 600 seconden wordt aan HA-kant als 60 behandeld.

De JavaScriptprobe bevestigt:

- Escape zonder geopende settings roept load_dashboard aan;
- interval 1 passeert de settings-frontend ondanks minimum 10 in HTML;
- acht onvertaalde sensorcategorieën;
- dashboardcreatiefout na registratie laat setup verborgen;
- het werkelijke injectiescript schrijft het synthetische token op een vreemde origin.

## Dependencycontroles opnieuw uitvoeren

Vanaf `desktop-app`:

```powershell
npm audit --json --package-lock-only
$env:COREPACK_ENABLE_AUTO_PIN = '0'
yarn audit --json
```

Vanaf `desktop-app/src-tauri`, wanneer het tijdens de audit lokaal gebouwde gereedschap nog aanwezig is:

```powershell
./target/audit-tools/bin/cargo-audit.exe audit --json --file Cargo.lock
```

De database verandert. De vastgelegde JSON/JSONL-bestanden zijn daarom de momentopname van deze audit. Scans mogen bij een volgende run andere advisories aantonen. Review per advisory target, featuregebruik, getroffen functie en beschikbare patch; forceer niet blind alle transitieve dependencies naar onverenigbare versies.

## Niet uitgevoerde checks

Geen echte HA-instance, HA fixture-suite, installer-smoketest, OS-shutdown/sign-out/sleep, macOS/Linux-build, releasebuild/signing/notarization, native webview/navigatietest, UI-screenshot/schermlezertest of performancebenchmark. Er is geen release gestart, commit gemaakt of code gepusht. Het onderzoek deed geen verzoeken naar een HA-server van de gebruiker.

## Bekende werkruimte-effecten

Frontend `dist`, Cargo-buildcache en audit-toolbinary zijn genegeerde gegenereerde bestanden. Corepack's automatische wijziging van package.json is exact teruggedraaid. Bestaande integratie-`__pycache__`-mappen zijn behouden. Tijdens het onderzoek verscheen een wijziging in integratie-`.gitignore` die `__pycache__` uitsluit; deze is ongemoeid gelaten. Nieuwe auditbestanden staan uitsluitend onder `docs/audit/2026-09-23`.
