# Portage EDC15C4 (BMW DDE 4.0 — M57 / M47 common rail)

Support de la famille Bosch EDC15C4 dans le moteur de détection de ZedSuite.

**État : squelette calibré sur UN dump.** Deux familles de maps sont
confirmées par la physique du fichier (les six durées d'injection, la
consigne de suralimentation). Le module est routé (`ecu_identifier.rs`,
`smart_detector.rs`) mais reste désactivé côté app (`ecus.json` :
`status: skeleton`, `enabled: false` ; `SUPPORTED_ECUS` commenté) tant que
la table des familles n'a pas été confrontée à une référence.

Fichier de référence : BMW E39 525d (M57D25, 163 ch), lecture 512 Ko,
en-tête `TSW V2.40 090799 1418 C4B/ESB/43`, logiciel Bosch `1037351632`
(second numéro `1037351761` dans le bloc de calibration).

---

## Ce qu'on sait du fichier

### Disposition (512 Ko, flash 29F400, C167 little-endian)

```
0x00000..0x08000   remplissage C3
0x08000            en-tête ASCII "TSW V2.40 090799 1418 C4B/ESB/43"
0x08020..0x40000   code programme
0x40000..0x50000   FF (effacé)
0x50000..0x60000   code programme
0x60000..0x70000   FF (effacé)
0x70000            bloc de calibration signé : 0x70001 = 67 FF FF FF FF FF FF "V2.0"
0x71800..0x7B200   les maps (enregistrements auto-descriptifs, voir plus bas)
0x72700..0x737D0   table de codes défaut (enregistrements de 0x38 octets
                   commençant par 10 0F 00 0F 86 DC), pas des maps
0x7BFB0            numéro Bosch "1037351761"
0x7BFFC            somme de contrôle 4 octets, fin du bloc à 0x7C000
0x7C000..0x7EC00   données non identifiées (pas d'axes auto-descriptifs)
0x7FEF0            numéro Bosch "1037351632"
0x7FF00            table de pointeurs AA 05 … 55 AA, puis C3 jusqu'à la fin
```

C'est exactement la forme du bloc « V4.1 » des EDC15P VAG (signature un
octet après le début, 0xC000 octets, somme à la fin), une révision plus
ancienne du format de bloc de code Bosch. **L'algorithme de la somme n'est
pas connu** : le correcteur VAG v4.1 (`edc15p-checksum.ts`) ne s'applique
pas et le frontend l'écarte explicitement pour ce type
(`src/lib/ecu-family.ts`, `isEdc15c4`). Le fichier est rendu intact ; il
faut un outil externe pour la somme tant que ce point n'est pas résolu.

Aucune chaîne `0281…` (numéro matériel Bosch) dans le fichier, aucune
référence VAG : c'est ce qui laissait le fichier `Unknown` avant ce portage
et ce qui, ajouté à la signature V2.0 et au jeton `C4x` de l'en-tête TSW,
l'identifie positivement maintenant.

### Les maps sont auto-descriptives

Même format d'enregistrement que sur EDC15P/EDC15VM :

```
[id u16][n u16][n × u16 valeurs d'axe, strictement croissantes]   premier axe
[id u16][m u16][m × u16 valeurs d'axe, strictement croissantes]   second axe (2D)
[n × m × u16 données]                                             ligne = PREMIER axe
```

Le second axe est l'indice rapide : `[rail 15][IQ 32][données]` = 15 lignes
de 32 valeurs. L'app affiche donc `rows = premier axe` (`y_axis_address`)
et `cols = second axe` (`x_axis_address`), l'ordre fichier est l'ordre
d'affichage, aucune transposition.

Une courbe 1D est `[id][n][axe][n × données]`.

L'octet haut de l'id est dans 0x80..0xFF (C0/C1/C2/C3, D9..DC, 96, DF sur
ce fichier). **L'id ne nomme pas la grandeur physique** : `C016` porte un
régime sur la plupart des maps, mais aussi `14..23` ou `205..1100` sur
d'autres. Les familles se reconnaissent donc par grille + plages de valeurs
d'axes + plages de données, jamais par id seul (`edc15c4/mod.rs`,
`classify`).

Le parcours (`layout.rs`, `parse_records`) consomme chaque enregistrement
entier avant de reprendre, donc un bloc de données ne peut pas être pris
pour un en-tête. Il trouve 209 enregistrements sur le fichier de référence
(`cargo test --test edc15c4_dump prints_the_record_inventory -- --ignored
--nocapture` les liste tous avec la famille reconnue).

---

## Familles calibrées (exposées par `detect()`)

| Famille | Enreg. | Données | Grille | Axes | Données |
|---|---|---|---|---|---|
| Injector duration 00 | 0x075B84 | 0x075BEA | 15 × 32 | Y rail 0,1 bar (119..1450), X IQ 0,01 mg/st (0..70) | µs, 0..5000 |
| Injector duration 01 | 0x075FAA | 0x076010 | 15 × 32 | idem | idem |
| Injector duration 02 | 0x0763D0 | 0x076436 | 15 × 32 | idem | idem |
| Injector duration 03 | 0x0767F6 | 0x07685C | 15 × 32 | idem | idem |
| Injector duration 04 | 0x076C1C | 0x076C82 | 15 × 32 | idem | idem |
| Injector duration 05 | 0x077042 | 0x0770A8 | 15 × 32 | idem | idem |
| Boost target map | 0x074292 | 0x0742CE | 16 × 10 | Y régime (0..4600), X IQ 0,01 mg/st (0..50) | mbar absolus, 918..2220 |

Pourquoi on les tient pour sûres, sans référence externe :

* **Durées** : six enregistrements contigus de même grille ; 0 µs à IQ 0
  sur chaque ligne ; croissantes le long de l'IQ ; à IQ donné, décroissantes
  quand la pression rail monte ; plafond 5000 sur la ligne 120 bar. À
  1200 bar et 50 mg/st la valeur est 902 µs, ordre de grandeur d'un
  injecteur CR de première génération. Six durées = une par emplacement
  d'injecteur, invariant de tous les EDC15 Bosch. Si le fichier n'en donne
  pas exactement six, `detect()` n'en montre aucune (une liste partielle
  serait prise pour complète).
* **Consigne de boost** : 990..1030 mbar à bas régime sans charge, soit la
  pression atmosphérique, 2195..2220 mbar à pleine charge entre 2500 et
  3500 tr/min (1,2 bar relatif, valeur constructeur du 525d 163 ch), retombe
  à 1860 à 4600. Aucune autre map du bloc n'a cette signature.

L'unité µs des durées est **déduite** (brut × 1, plausible) ; le facteur
0,01 mg/st de l'axe IQ vient de la plage 0..7000 pour une pleine charge
attendue vers 50 mg/st. À confirmer sur damos ou pack WinOLS.

Rapport de complétude (`commands.rs`, `build_expected_report_edc15c4`) :
6 durées, 1 consigne de boost. À étendre à chaque promotion de famille.

---

## Hypothèses (dans `inventory()`, jamais dans `detect()`)

| Enreg. | Grille | Axes | Données | Hypothèse |
|---|---|---|---|---|
| 0x0745E4 | 16 × 10 | régime × IQ (0..60) | 8500 → 2705 avec le régime | Duty de base actionneur VNT (0,01 %) — ou EGR, même forme |
| 0x07A2DE, 0x07AAD6 (copie identique), 0x07AD1E | 16 × 16 | régime × `C20C` 2000..8500 | 1701..6289, plateau par ligne | Conversion demande → IQ avec limiteur intégré (`C20C` = couple 0,1 Nm ?) |
| 0x071978, 0x071A62 | 1D | CAN 10 bits (46..1023) | 4131 → 2331 | Linéarisation capteur NTC (0,1 K) |

Autres enregistrements notables, non classés :

* **Pression rail** : `C032` 15 points 1190..14500 est l'axe rail des
  durées ; 0x075840 (8 × 8, régime × `C030` 2000..13500, données
  1100..4300) et 0x077D5A / 0x078882 (1D sur `C032`, 18..268) tournent
  autour de la pression rail sans qu'on sache lesquels sont la consigne.
* **Températures** : `C156` (liquide de refroidissement) et `C15E`
  (air / carburant) sont en 0,1 K (2731 = 0 °C). Une trentaine de courbes
  1D à 10000 = 100 % : corrections par température.
* **Trois courbes régime 19 points** 0x07A8BA / 0x07A910 / 0x07A960
  (5000, 3900, 3400 … 5250) : allure d'un limiteur d'IQ par régime, trois
  variantes (sélecteur ?). 0x0737D6 (3800 → 3000) est peut-être le limiteur
  de couple.
* **Régime × température** 0x07236C (8 × 8, 450..1755) et 0x07A540
  (8 × 8, 0..3000) : ralenti par température ? IQ de démarrage ?
* 0x079C3C / 0x079D2C / 0x079E1C (12 × 8, `C016` 0..4845 × `C1C8`
  118..10000) : trois variantes d'une même map, 0..6568.
* `DA..` : axes CAN 10 bits (0..1023), tables de linéarisation capteurs.

Rien de tout cela ne doit passer dans `detect()` sans une référence : un
nom faux est pire qu'une map absente (CONTRIBUTING.md).

---

## Ce qu'il faut pour sortir du squelette

1. **Une référence** : damos / A2L DDE 4.0, pack WinOLS M57, ou au minimum
   un second dump d'un autre logiciel (530d, 320d) pour voir ce qui bouge.
   Avec elle, promouvoir les hypothèses une par une (`Family::calibrated`).
2. **La somme de contrôle** du bloc V2.0 : sans elle l'app ne peut pas
   écrire un fichier flashable. Comparer un original et un fichier corrigé
   par un outil connu.
3. **Un corpus** au sens de CONTRIBUTING.md (stock et modifiés, M57 et M47,
   plusieurs années) et le banc `dump_maps` dessus.
4. Seulement alors : `enabled: true`, `SUPPORTED_ECUS`, statut `beta`.

## Garde-fous posés par ce portage

* `identify_bmw_edc15c4` tourne AVANT la logique EDC15 VAG et exige
  512 Ko + signature V2.0 + en-tête `TSW … C4x` + aucune référence VAG ni
  signature V4.1. Un fichier V2.0 sans en-tête reste `Unknown`.
* `smart_detector.rs` route `EDC15C4` vers son propre détecteur, jamais
  vers `EDC15PDetector`.
* Frontend : `isVagEdc15` / `isEdc15c4` (`src/lib/ecu-family.ts`) sortent
  EDC15C4 du correcteur de somme VAG, des DTC EDC15P et du launch control.
* `detect()` refuse toute taille autre que 512 Ko et tout fichier sans bloc
  V2.0.

## Tests

* `cargo test edc15c4` : parseur (synthétique), règles de classification,
  identification positive et négative.
* `ZEDSUITE_C4_DUMP=… cargo test --test edc15c4_dump -- --ignored
  --nocapture` : identification, adresses attendues et inventaire sur le
  dump de référence (jamais commité).
