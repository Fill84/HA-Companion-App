# PawnIO artefacten voor de geïntegreerde Windows-sensorroute

Status: de meegeleverde driver en onze Rust-sensorservice zijn op `phill-pc` en `beast-unit` (Windows x64, Intel) geïnstalleerd en hebben echte CPU-temperatuurmetingen opgeleverd. Alleen op `phill-pc` is de volledige route tot Home Assistant en de kaart bevestigd. Een nieuwe publieke release vereist nog de actuele installer-/upgradeproef, langdurige stabiliteitsproef en de overige platform- en hardwarecontroles uit de verificatiestatus.

De x64- en ARM64-driverpakketten zijn zonder uitvoering uit de officiële [PawnIO.Setup 2.2.0-release](https://github.com/namazso/PawnIO.Setup/releases/tag/2.2.0) gehaald. SHA-256 van `PawnIO_setup.exe`: `1f519a22e47187f70a1379a48ca604981c4fcf694f4e65b734aaa74a9fba3032`. De Windows Authenticode-status van het setupbestand en beide catalogi was `Valid`; de catalogi zijn ondertekend door `Microsoft Windows Hardware Compatibility Publisher`. Alleen de ondertekende x64- en ARM64-blokken uit het CAB-archief zijn opgenomen, zonder de `unrestricted`-blokken. Controle bij build en installatie blijft vereist.

`signtool verify /kp /c` valideerde bovendien de `.sys`- en `.inf`-bestanden van beide architecturen tegen hun catalogus.

De Intel- en AMD-module komen uit de officiële [PawnIO.Modules 0.2.11-release](https://github.com/namazso/PawnIO.Modules/releases/tag/0.2.11). SHA-256 van `release_0_2_11.zip`: `43608cb89bc84247fef1368a139013f7d043e17db6d6c8dfc9b46bf0905a81f4`. Alleen IntelMSR wordt momenteel door de service gebruikt; AMDFamily17 is gepind maar AMD-uitlezing en fysieke validatie ontbreken nog.

| Bestand | SHA-256 |
|---|---|
| `x64/PawnIO.sys` | `fca6e7d58b0cf38dbb913a2b9e532f48629145d395f454b16a9f58e97b8d3940` |
| `x64/PawnIO.inf` | `7c1c203e13693531243fbee3cb87d7b79170eae89f5729b3f41387fe68a54f0b` |
| `x64/pawnio.cat` | `a37d46840280efec92063d3a21014c803939e599b4c0da4a4d063b79eeca9446` |
| `arm64/PawnIO.sys` | `8113d5850e4d7d2cbf7573b12a1de57f254f51b925b017e105fb558ce3a16600` |
| `arm64/PawnIO.inf` | `9989a2d6985f1131dc7618cb0a4e76f5e59c4837c5057ed765b80bc979ac00cb` |
| `arm64/pawnio.cat` | `d95848578d62d33d9eed30b7478d323611818ff6a38c43363aca58777e2f8d35` |
| `modules/IntelMSR.bin` | `d6ed85d65ab17a22f813ef98207d6d537155ee2ded5976a21cb48413c9b92e5f` |
| `modules/AMDFamily17.bin` | `dae74615761b78bdf064dfb3e136252ddcc6fc727d88f14738d0e5800d427a91` |

De driverbron voor 2.2.0 staat op [`namazso/PawnIO` tag `2.2.0`](https://github.com/namazso/PawnIO/tree/2.2.0), commit `5cdf470831fdfff3f7f1d06363ca6b230f3bf35a`; de modulebron staat op [`namazso/PawnIO.Modules`](https://github.com/namazso/PawnIO.Modules). Licenties: PawnIO GPL-2.0 met eigen uitzondering (`COPYING.PawnIO.txt`) en modules LGPL-2.1-or-later (`COPYING.Modules.txt`). Voor verspreiding moet het bijbehorende broncodeaanbod bij de release worden toegevoegd en juridisch gecontroleerd. De driver geeft alleen `SYSTEM` en `Administrators` toegang volgens de originele INF; verander deze beveiligingsinstelling niet om de GUI toegang te geven.
