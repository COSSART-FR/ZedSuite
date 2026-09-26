# Portage EDC16C31 (BMW DDE — E46 320d 150 ch)

Support de la famille Bosch EDC16C31 dans le moteur de détection de ZedSuite.

**État : bêta, calibré sur UN logiciel (trois fichiers).** 36 maps nommées,
adresses vérifiées sur la lecture d'origine et retrouvées à l'identique sur
les deux tunes du même soft. Le module est activé (`ecus.json` :
`status: beta`, `enabled: true`), l'export mappack reste désactivé. Le
checksum de la région de calibration est vérifié et corrigé.

Corpus :

| Fichier | Contenu | Checksum calibration |
|---|---|---|
| `origine` | lecture d'origine, E46 320d 150 ch (M47TU), SW Bosch `1037361858` | valide |
| `stage1-egroff` | stage 1 + EGR off du même soft | non corrigé (l'outil de flash le fait) |
| `stage1-egroff-hardcut` | idem + rupteur 5250 → 4300 tr/min | non corrigé |

Il n'y a **pas de damos** de ce soft dans le projet. Les noms viennent de la
structure des blocs, des valeurs physiques et du diff origine → stage 1 ; les
36 adresses de données ont ensuite été recoupées une par une avec une liste
publique de maps du même logiciel (ecuedit.com, « BMW e46 320d 150hp Maps »,
EDC16C31 SW 361858) : toutes tombent au même octet.

---

## Disposition du fichier (1 Mo, lu à partir de 0x800000)

```
0x000000..0x030000   boot + programme (la lecture s'arrête à 0x30000)
0x030000..0x0E0000   FF (non lu)
0x0E0000..0x100000   calibration
0x0E0254             "10373618581974842A"  (SW Bosch + référence BMW)
0x0E6358             "01EDC16  System EDC16XY"
0x0FE639             ",99/1/EDC16C31/999/X000/X/0X0000_000/..."
```

Le fichier est l'image de la flash mappée à `0x800000`. Les descripteurs de
checksum EDC16 (`FA DE CA FE CA FE AF FE`, précédés des adresses de début et
de fin INCLUSIVE en big-endian) donnent des adresses ECU : offset = adresse −
0x800000.

| Région (fichier) | Descripteur | Somme sur l'origine |
|---|---|---|
| `0x08000..0x0803F` | `0x082C4` | `0xD01FE500` (valide) |
| `0x08180..0x1FFF7` | `0x0829C` | `0xD01FE500` (valide) |
| `0x20000..0x5FFFB` | `0x2003C` | invalide : la lecture s'arrête à `0x30000` |
| `0xE0180..0xFEFFB` | `0xE0280` | `0xD01FE500` (valide) |

**La région de calibration est celle qui porte `/EDC16C31/`.** C'est elle que
le détecteur scanne, et la seule que le correcteur de checksum écrit.

### Pourquoi l'EDC16 VAG ne suffisait pas

1. **Identification.** Avant ce portage, le fichier sortait **EDC16U1 (75 %)**
   et partait au détecteur VAG, qui y « trouvait » 18 maps. Le portail
   `identify_bmw_edc16c31()` passe maintenant avant toute la logique EDC16.
2. **Checksum.** Même invariant que le VAG (somme des dwords = `0xD01FE500`,
   checksum dans le dernier dword), mais le descripteur n'est **pas** à
   `début + 0x3C` (région `0x8E0180`, descripteur à `0x8E0280`) : le module VAG
   déduit la base de cette position, tombe sur des bornes fausses et ne
   retient aucune région (« non supporté »). `edc16c31-checksum.ts` prend la
   base fixe `0x800000`.

---

## Format des maps : le même que l'EDC16CP31 Mercedes

Toutes les maps 2D sont des blocs Bosch `Kf_Xs16_Ys16_Ws16` :

```
+0x00  u16        nx          points du 1er axe
+0x02  u16        ny          points du 2e axe
+0x04  i16[nx]    1er axe     strictement croissant
+....  i16[ny]    2e axe      strictement croissant
+....  i16[nx*ny] données     2e axe = index rapide : z[x*ny + y]
```

et les courbes `Kl_Xs16_Ws16` : `[n][axe n][données n]`. Tout est big-endian.
Un bloc est alloué à sa taille maximale : une courbe de 19 points dans une
case de 21 est suivie de 8 octets de bourrage, et les points d'axe inutilisés
sont parfois remplis de valeurs croissantes factices (4510, 4520…).

Affichage : `lignes = 1er axe` (`y_axis_address`), `colonnes = 2e axe`
(`x_axis_address`), `as_stored: true`. Ordre fichier = ordre affiché, les
règles de transposition par nom du frontend (VAG) ne s'appliquent pas.

Le scan lit la région de calibration sur les offsets pairs, consomme chaque
bloc `Kf` valide en entier (en-tête plausible + deux axes strictement
croissants), puis cherche les courbes `Kl` dans les trous entre les maps :
233 blocs `Kf` et 287 courbes sur l'origine.

---

## Les familles

Adresses **de données** sur l'origine (SW `1037361858`) :

| Famille | Nb | Données | Grille | Axes (lignes / colonnes) | Facteur | Unité |
|---|---|---|---|---|---|---|
| Driver wish | 2 | `E0674`, `E08B8` | 16×10 | régime / pédale | 0.1 | Nm |
| Smoke limiter (boost pressure) | 1 | `E13AE` | 12×16 | pression de suralim. / régime | 0.01 | mm³/st |
| Torque limiter | 1 | `E66DA` | 21 | régime | 0.1 | Nm |
| Torque limiter 1..5 | 5 | `E6DC2` … `E6F1A` | 21/19 | régime | 0.1 | Nm |
| Torque to IQ 1..3 | 3 | `E80EA`, `E832E`, `E8572` | 16×16 | régime / couple | 0.01 | mm³/st |
| Main injection SOI 1..5 | 5 | `EA80C` … `EB11C` | 16×16 | régime / IQ | 3/128 | °CA |
| Injector duration 1..6 | 6 | `F5364` … `F6958` | 16×32 | pression rail / IQ | 1 | µs |
| Boost actuator duty map 1..5 | 5 | `F7F44` … `F8900`, `FA3DC` | 16×16 | régime / IQ | 100/8192 | % |
| Boost target map 1..3 | 3 | `F9394`, `F95D8`, `F981C` | 16×16 | régime / IQ | 1 | mbar |
| Boost limiter (atmospheric pressure) | 1 | `F9A48` | 10×10 | régime / pression atmo | 1 | mbar |
| Rail pressure limiter 1..2 | 2 | `FBCD6`, `FC75A` | 16×16 | régime / consigne rail | 0.1 | bar |
| Rail pressure target map 1..2 | 2 | `FC9D0`, `FCE64` | 16×16 | régime / IQ | 0.1 | bar |

Chaque famille n'est rapportée que si le fichier répond avec son jeu
complet : exactement 2 driver wish, 3 torque to IQ, 5 SOI **adjacentes**, 6
durations **adjacentes**, 3 boost targets, 5 duty, 2 rail targets, 2 rail
limiters, 5 courbes de limiteur **contiguës**, et exactement un smoke
limiter, un boost limiter, un limiteur principal. Sinon la famille est
abandonnée entière. Aucune règle ne lit une adresse.

### Conversions et ce qui les appuie

| Grandeur | Brut → physique | Vérification sur l'origine |
|---|---|---|
| régime | ×1 tr/min | axes 250..5020, coupure à 5020 |
| quantité | ×0.01 | axes jusqu'à 70..85 |
| couple | ×0.1 Nm | driver wish −45..+377 Nm, limiteur 368 Nm en crête |
| pédale | ×100/8192 % | l'axe finit exactement à 8192 (`Prc` Bosch) |
| rapport cyclique | ×100/8192 % | N75 15..83 % |
| pression rail | ×0.1 bar | consignes 300..1605 bar, SVRL 1600 bar |
| suralimentation | ×1 hPa | 990 hPa au ralenti, 2400 hPa en pleine charge |
| avance | ×3/128 °CA | 2..32 °CA avant PMH |
| temps d'injection | ×1 µs | 1211 µs pour 85 à 1600 bar, plafond 5000 µs |

⚠ **L'unité de quantité n'est pas confirmée.** Elle est affichée en
**mm³/coup** comme dans les A2L Bosch des deux projets voisins (DDE 4.0 de la
E39 et CP31 Mercedes, même format de blocs). Un bilan énergétique sur la map
Torque to IQ (350 Nm → 56,2 unités à 2000 tr/min) colle un peu mieux à des
**mg** (rendement indiqué 45 % en mg, 55 % en mm³). Seul un damos BMW EDC16
tranchera. Le facteur 0.01 n'est pas en cause, seulement le libellé.

### Seuils de valeurs

L'app détecte **tous** les fichiers en mode stock (le `tuned_mode` n'est
jamais passé par le frontend). Les plafonds « stock » laissent donc déjà la
place à un remap courant (boost 4000 hPa, rail 2000 bar, limiteurs 800 Nm) :
ce qui distingue une famille ici, ce sont ses axes et son jeu, pas ses
valeurs. Les deux tunes du corpus sortent les 36 maps en mode stock.

---

## Ce que fait le stage 1 du corpus (diff origine → stage1-egroff)

| Zone | Map | Modification |
|---|---|---|
| `E2A22`, `E2B54` | courbes « EGR hysteresis » (non exposées) | mises à zéro → **EGR off** |
| `E66DA` | Torque limiter | crête 322 Nm → 650 Nm |
| `E6DC2..E6F1A` | Torque limiter 1..5 | plafonnés à 600 Nm |
| `EA80C..EB11C` | SOI 1..5 | avance +0,4..3,2 °CA en charge |
| `F5364..F6958` | Injector duration 1..6 | +2..19 % en haut de quantité |
| `F9394..F981C` | Boost target 1..3 | +120 hPa en charge |
| `F9A48` | Boost limiter atmo | +120 hPa |
| `F9C60` | SVBL (valeur seule, non exposée) | 2400 → 2520 hPa |
| `FBCD6`, `FC75A` | Rail pressure limiter 1..2 | dernière colonne 1600 → 1660 bar |
| `FC9D0`, `FCE64` | Rail pressure target 1..2 | +40..70 bar en charge (max 1605 → 1675 bar) |
| `FD18C` | SVRL (valeur seule, non exposée) | 1600 → 1670 bar |

Le driver wish et les maps Torque to IQ ne sont **pas** touchés : le gain passe
par les limiteurs, la suralimentation et surtout les **temps d'injection**
(plus de carburant réel pour la même quantité demandée). Le hardcut change un
seul mot, à `0xE6D5E` : 5250 → 4300 tr/min (valeur seule, non exposée).

---

## Ce qu'il reste à faire

1. **Un deuxième logiciel.** Tout est calibré sur un seul soft. Les jeux
   (2 driver wish, 5 SOI, 6 durations…) peuvent être propres à ce soft ; un
   deuxième fichier dira ce qui appartient au calculateur. Le module est aussi
   probablement proche de ce qu'il faut pour les M57TU (330d, 530d) en
   EDC16C31, mais rien n'a été vérifié.
2. **L'unité de quantité** (mm³ ou mg), voir plus haut.
3. **Les valeurs seules** : SVBL (`F9C60`), SVRL (`FD18C`), rupteur (`E6D5E`).
   Ce ne sont pas des blocs auto-décrits ; les trouver sans adresse fixe
   demande une ancre structurelle qu'on n'a pas encore.
4. **L'EGR.** Les courbes mises à zéro par le EGR off (25 points, régime avec
   points factices 4510..4650) sont visibles mais leur sens n'est pas établi :
   pas exposées.
5. **Smoke limiter par débit d'air** (`E15FC`, 16×16 régime × IQ, 480..1260
   ×0.1) et **table lambda** (`E7A16`) : listées par la référence publique,
   sens et unités non confirmés, pas exposées.
6. **L'estimation de puissance** ne connaît pas ce calculateur : sur
   l'origine elle donne ≈115 ch au lieu de 150, et ne voit pas le gain du
   stage 1 (qui passe par les durations, que le modèle ne lit que sous les
   noms VAG « Duration 00/01 »). À ne pas croire tant qu'elle n'est pas
   calibrée.
7. **DTC** : désactivé pour EDC16C31 (les tables de contrôle ne sont pas
   celles des EDC16 VAG).

---

## Tests

`cargo test --lib edc16c31` : lecture d'un bloc `Kf` en ordre colonne,
refus d'axes non croissants / de flash effacée, régions de checksum à la base
`0x800000`, familles sur blocs synthétiques (jeux incomplets abandonnés, SOI
adjacentes, courbes contiguës, blocs hors région ignorés), identification
positive et négatives (chaîne famille sans région, pièce VAG, fichier 2 Mo).

Banc sur les vrais fichiers (non versionnés), `#[ignore]` :

```sh
ZEDSUITE_C31_DUMP_320D=/chemin/origine \
ZEDSUITE_C31_DUMP_320D_TUNED=/chemin/stage1-egroff \
  cargo test --test edc16c31_dump -- --ignored --nocapture
```

Liste attendue : `tests/fixtures/edc16c31/320d-1037361858.json`. Le test
vérifie noms, adresses, grilles, et que chaque axe exporté est strictement
croissant et plausible pour son libellé.

Non-régression : `dump_maps` sur les 31 autres fichiers du banc (A3 1.9 TDI,
Leon 1.9 TDI, Golf 5 1.9 TDI, E39 525d + 7 versions, CLK 320 CDI) avant et
après : sorties JSON identiques à l'octet.
