# Portage EDC15C4 (BMW DDE 4.0 — M57 / M47 common rail)

Support de la famille Bosch EDC15C4 dans le moteur de détection de ZedSuite.

**État : bêta, calibré sur deux damos Bosch et trois builds.** 36 maps
nommées d'après l'A2L du projet Bosch P079.VB4, adresses vérifiées sur les
trois fichiers du corpus. Le module est activé dans l'app (`ecus.json` :
`status: beta`, `enabled: true`), l'export mappack reste désactivé. **La
somme de contrôle du bloc n'est pas connue** : l'app exporte le fichier
sans correction (statut « unsupported » du checksum, comme sur MJD6), à
corriger avec un outil externe avant flash.

Corpus :

| Fichier | Build | Soft Bosch | Origine |
|---|---|---|---|
| lecture E39 525d (M57D25, 163 ch) | `TSW V2.40 090799 1418 C4B/ESB/43` | 1037351632 | lecture flash réelle |
| `4ZB1379_TSW-V2.40_0090799.ori` | même en-tête TSW | 1037351513 | binaire du damos 4ZB1379 (A2L + .dam, généré 24-08-2000) |
| `6ZC1179_530D.org` | en-tête effacé | — | binaire du damos 6ZC1179 (A2L), 530d |

Les deux A2L décrivent 4434 / 4470 caractéristiques (115 MAP, 133 CURVE,
le reste VALUE). 197/203 et 203/205 de leurs MAP/CURVE à axes intégrés
tombent exactement sur un enregistrement du parseur (`layout.rs`) dans
leur binaire : l'A2L et le .ori sont bien le même build, et le format
d'enregistrement est celui qu'on lit.

---

## Disposition du fichier (512 Ko, flash 29F400, C167 little-endian)

```
0x00000..0x08000   remplissage C3
0x08000            en-tête ASCII "TSW V2.40 090799 1418 C4B/ESB/43" (absent sur un .ori de damos)
0x08020..0x40000   code programme
0x40000..0x50000   FF (effacé)
0x50000..0x60000   code programme
0x60000..0x70000   FF (effacé)
0x70000            bloc de calibration signé : 0x70001 = 67 FF FF FF FF FF FF "V2.0"
0x71800..0x7B200   les maps (≈210 enregistrements auto-descriptifs + le groupe injection)
0x72700..0x737D0   table de codes défaut (enregistrements de 0x38 octets), pas des maps
0x7BFB0            numéro Bosch (second numéro)
0x7BFFC            somme de contrôle 4 octets, fin du bloc à 0x7C000
0x7FEF0            numéro Bosch (logiciel)
0x7FF00            table de pointeurs AA 05 00 00 00 00 55 AA …, puis C3
```

Même forme que le bloc « V4.1 » des EDC15P VAG (signature un octet après
le début, 0xC000 octets, somme à la fin), révision plus ancienne du format
de bloc Bosch. L'algorithme de la somme reste à identifier ; le correcteur
VAG v4.1 ne s'applique pas et le frontend l'écarte pour ce type
(`src/lib/ecu-family.ts`, `isEdc15c4`).

## Identification (`ecu_identifier.rs`, `identify_bmw_edc15c4`)

Toujours : 512 Ko, signature V2.0 présente, aucune signature V4.1, aucune
référence VAG. Puis :

* en-tête `TSW V… C4x/…` dans les 64 premiers Ko → confiance 0,88 ;
* sinon, preuve structurelle pour un fichier à en-tête effacé (les .ori de
  damos) : V2.0 exactement à 0x70001, marqueur `AA 05 00 00 00 00 55 AA`
  à 0x7FF00, au moins 100 enregistrements dans le bloc → confiance 0,75.

La passe tourne AVANT la logique EDC15 VAG. Un fichier V2.0 sans en-tête
ni structure reste `Unknown`.

## Deux formats de maps

### 1. Enregistrements auto-descriptifs (`layout.rs`)

Record layouts A2L `MSA15_KL1xx` / `MSA15_KF3xx` :

```
[SRC_ADDR_X u16][n u16][n × u16 axe X, strictement croissant]     premier axe
[SRC_ADDR_Y u16][m u16][m × u16 axe Y, strictement croissant]     second axe (2D)
[n × m × u16 données]   FNC_VALUES COLUMN_DIR : ligne = PREMIER axe
```

`SRC_ADDR` est l'**adresse RAM de la grandeur d'entrée** (C016 =
`dzmNmit`, régime), pas un type physique : 30 des 201 enregistrements
nommés par les deux A2L ont un id différent d'un build à l'autre. Les
règles (`families.rs`) lisent donc grille + plages des axes + plages des
données + ordre dans le fichier, jamais un id seul ni une adresse.

L'app affiche `rows = premier axe` (`y_axis_address`) et `cols = second
axe` (`x_axis_address`), ordre fichier = ordre d'affichage. Une courbe 1D
porte son axe dans `x_axis_address`.

### 2. Maps de groupe du bloc injection (`group.rs`)

Record layout `MSA15_GKF4` : données nues, axes partagés (`AXIS_PTS`
`zuwXstzv`, `zuwYstzv*`, `zuwYEakt*`). Les huit axes forment un **cluster**
de forme fixe [16, 6, 4, 6, 8, 16, 8, 16] et les maps sont à **offset fixe
du cluster**, identiques sur les trois builds (A2L à l'appui) :

| Offset | A2L | Grille | Axes | Map |
|---|---|---|---|---|
| +0x000 | zuwXstzv | 16 | régime | axe X partagé (valeurs à +0x004) |
| +0x050 | zuwYstzv8 | 8 | mm³ M_EMTS | (valeurs à +0x054) |
| +0x064 | zuwYstzv16 | 16 | mm³ M_EMTS | (valeurs à +0x068) |
| +0x088 | zuwYEakt8 | 8 | mm³ M_EAKT | (valeurs à +0x08C) |
| +0x09C | zuwYEakt16 | 16 | mm³ M_EAKT | (valeurs à +0x0A0) |
| +0x0F0 | zuwPQGWKF | 16×16 | Xstzv × YEakt16 | Rail pressure target map |
| +0x6F0 | zuwPQmaxKF | 16×8 | Xstzv × YEakt8 | Rail pressure maximum map |
| +0x2542 | zuwABVGWKF | 16×16 | Xstzv × Ystzv16 | Pilot injection SOI (relative) |
| +0x2A3E | zuwMEVGWKF | 16×16 | Xstzv × Ystzv16 | Pilot injection quantity |
| +0x2D22 | zuwMVEmxKF | 16×8 | Xstzv × Ystzv8 | Pilot injection quantity maximum |
| +0x2E66 | zuwABHmxKF | 16×8 | Xstzv × Ystzv8 | Main injection SOI earliest |
| +0x2F66 | zuwABHG1KF | 16×16 | Xstzv × Ystzv16 | Main injection SOI (with pilot) |
| +0x3166 | zuwABHG2KF | 16×16 | Xstzv × Ystzv16 | Main injection SOI (no pilot) |

La disposition est rigide jusqu'à +0x39C2 sur les deux A2L (les builds
divergent de 0x44 octets après). Chaque bloc est quand même validé sur
une fenêtre physique avant d'être rapporté ; deux clusters dans un
fichier = groupe refusé.

## Unités (COMPU_METHOD de l'A2L)

| A2L | Unité | brut → physique | Où |
|---|---|---|---|
| N | tr/min | ×1 | axes régime |
| MM3 | mm³/coup | ×0,01 | axes IQ, données quantité |
| M_L | mg/coup d'air | ×0,1 | axes débit d'air (limiteurs de fumée), données EGR |
| RP | hPa rail | ×100 hPa = ×0,1 bar | axe rail des durées, maps de pression rail |
| P | hPa | ×1 (= mbar) | consigne de boost, axe pression atmosphérique |
| PROZ / PROZ_S | % | ×0,01 | duty, axe pédale |
| AD_uS | µs | ×1 | durées d'injection |
| GradKW | °vilebrequin | ×0,0234375, signé | SOI |
| T | °C | ×0,1 − 273,14 | axes température |

**Les quantités de ce calculateur sont des volumes (mm³/coup), pas des
masses.** Les libellés le disent ; aucune conversion en mg n'est faite.
Conséquence : l'estimation de puissance (`power-estimation.ts`), écrite en
mg/coup, est ~16 % optimiste sur cette famille (densité gazole ≈ 0,835)
tant qu'une correction n'est pas ajoutée.

## Les 36 maps (banc : 36/36 aux adresses A2L sur 4ZB1379 et 6ZC1179)

| Nom ZedSuite | A2L | Grille | Y (lignes) × X (colonnes) | Z |
|---|---|---|---|---|
| Injector duration 10/11/12 (no pilot) | zuwAD_KF10..12 | 15×32 | rail (bar) × IQ | µs |
| Injector duration 20/21/22 (with pilot) | zuwAD_KF20..22 | 15×32 | rail × IQ | µs |
| Rail pressure target map | zuwPQGWKF | 16×16 | régime × IQ | bar |
| Rail pressure maximum map | zuwPQmaxKF | 16×8 | régime × IQ | bar |
| Pilot injection quantity | zuwMEVGWKF | 16×16 | régime × IQ | mm³ |
| Pilot injection quantity maximum | zuwMVEmxKF | 16×8 | régime × IQ | mm³ |
| Main injection SOI (with pilot) | zuwABHG1KF | 16×16 | régime × IQ | °CA signé |
| Main injection SOI (no pilot) | zuwABHG2KF | 16×16 | régime × IQ | °CA signé |
| Main injection SOI earliest | zuwABHmxKF | 16×8 | régime × IQ | °CA signé |
| Pilot injection SOI (relative) | zuwABVGWKF | 16×16 | régime × IQ | °CA |
| Boost target map (eco) | ldwSWoekKF | 16×10 | régime × IQ | mbar abs |
| Boost actuator duty base map (eco) | ldwTVoekKF | 16×10 | régime × IQ | % |
| Boost actuator duty base map (sport) | ldwTVspoKF | 2×2 | régime × IQ | % |
| Boost actuator duty limit (max) / (min) | ldwGRmaxKF / ldwGRminKF | 16×4 | régime × IQ | % |
| Smoke limiter (dynamic) | mrwBRDY_KF | 16×16 | régime × débit d'air | mm³ |
| Smoke limiter | mrwBRA_KF | 16×16 | régime × débit d'air | mm³ |
| Smoke limiter (low range) | mrwBRLWRKF | 16×16 | régime × débit d'air | mm³ |
| Smoke limiter correction 1 / 2 | mrwBRAkAKF / mrwBRAkLKF | 11×12 | régime × débit d'air | mm³ signé |
| Driver wish 1 (low range) / 2 (lower v_nenn) / 3 (upper v_nenn) | mrwFVLR_KF / FVHU_KF / FVHO_KF | 12×8 | régime × pédale (%) | mm³ |
| Torque limiter (pull-away) / (raised) / (normal) | mrwADB_KL / BDBH_KL / BDBN_KL | 19 | régime | mm³ |
| Torque limiter (low range) | mrwBDBLRKL | 16 | régime | mm³ |
| Turbo protection full-load quantity | mrwLDNB_KF | 10×9 | régime × P atmo (mbar) | mm³ |
| Full-load raise by coolant temperature | mrwBWT_KF | 8×8 | régime × T eau | mm³ |
| Start quantity base map | mrwSTMGRKF | 8×10 | régime démarrage × T eau | mm³ |
| EGR air mass target map | arwMLGRDKF | 12..14×16 | régime (700..2700) × IQ | mg/coup |
| EGR duty base map | arwDraTVKF | 8×8 | IQ × régime (≤ 3000) | % |

Le jumeau « sport » de la consigne de boost (ldwSWspoKF, 2×2) vaut 0 sur
les trois fichiers : programme inutilisé, la règle existe mais ne le
rapporte pas.

Ce qui distingue les jumeaux de même grille :

* les trois limiteurs de fumée 16×16 régime × débit d'air : ordre fichier
  (dynamique, principal, low range), identique sur les trois builds ;
  n'importe quel autre compte que 3 → aucun rapporté ;
* la remontée de pleine charge (mrwBWT_KF, 8×8 régime × T eau) : entre le
  limiteur dynamique et les deux corrections ; la map de frottements
  (mrwREI_KF, même grille, mêmes axes) est bien plus tôt dans le bloc ;
* les trois courbes de limitation de couple 19 points : une suite de trois
  enregistrements consécutifs, puis la courbe 16 points low range ;
  mrwBEM_KL / mrwANFXUKL / mrwANFAHKL (19 points aussi) ne sont jamais
  trois d'affilée et ne sont pas rapportées ;
* le duty EGR (arwDraTVKF, 8×8 IQ × régime) : suivi de la courbe de
  linéarisation du débitmètre (32 points) ; la map de post-injection
  (zuwANEGKF, même forme) ne l'est pas.

## Rapport de complétude (`commands.rs`, `build_expected_report_edc15c4`)

Invariants du calculateur : 6 durées, 1 consigne rail, 2 SOI principales,
1 quantité pilote, 1 consigne de boost (eco), 1 duty de base (eco), 3
limiteurs de fumée, 3 driver wish, 3 limiteurs de couple, 1 consigne EGR.

## Ce qui reste

1. **La somme de contrôle** du bloc V2.0 (0x7BFFC) et de la zone
   0x7C000–0x7FFFF : un original + le même corrigé par un outil connu.
2. Un corpus au sens de CONTRIBUTING.md : des lectures réelles (M57 et
   M47, plusieurs années, des fichiers modifiés). Les trois fichiers du
   banc sont tous d'origine.
3. Familles vues dans l'A2L et pas encore exposées : ralenti par
   température (mrwLTW_KL), courbes de pression rail min / limite
   (zuwPQminKL, zuwPQ_mnKL), corrections de SOI et de quantité pilote par
   température (zuwABH*kKF, zuwMVE*kKF, GKF à +0x344A…), consigne EGR
   corrigée (arwTLKORKF, arwTWKORKF), limiteur de vitesse (VALUE dans
   l'A2L, pas une map).
4. Estimation de puissance : facteur de densité pour les mm³.
5. Zone 0x7C000–0x7EC00 : données sans axes auto-descriptifs, non
   explorée.

## Tests

* `cargo test edc15c4` : parseur, cluster, règles de classification sur
  blocs synthétiques, identification positive et négative (VAG V4.1,
  fichier sans en-tête, V2.0 seul).
* Banc sur les vrais fichiers, listes attendues dans
  `src-tauri/tests/fixtures/edc15c4/` (adresses A2L pour les deux damos) :

  ```
  ZEDSUITE_C4_DUMP_4ZB1379=… ZEDSUITE_C4_DUMP_6ZC1179=… ZEDSUITE_C4_DUMP_525D=… \
    cargo test --test edc15c4_dump -- --ignored --nocapture
  ```

  `prints_the_record_inventory` liste chaque enregistrement avec la
  famille reconnue : c'est la vue pour promouvoir une nouvelle famille.
