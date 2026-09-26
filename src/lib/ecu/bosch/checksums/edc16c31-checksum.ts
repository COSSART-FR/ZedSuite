/**
 * EDC16C31 Checksum (Bosch EDC16C31, BMW DDE — 1 Mo lus à 0x800000)
 *
 * Même invariant que les EDC16 VAG (edc16-checksum.ts) : la somme des
 * dwords big-endian d'une région, mot de checksum inclus, vaut 0xD01FE500,
 * et le checksum est le DERNIER dword de la région. Les régions sont
 * annoncées par la même signature FA DE CA FE CA FE AF FE, précédée des
 * adresses ECU de début et de fin (INCLUSIVE).
 *
 * Ce qui change sur BMW, et pourquoi le module VAG rend « non supporté » :
 * le descripteur n'est PAS à `début + 0x3C` (région calibration 0x8E0180,
 * descripteur à 0x8E0280), donc la base mémoire ne se déduit pas de la
 * position du descripteur. Elle est fixe : 0x800000 pour une lecture 1 Mo.
 *
 * Vérifié sur la lecture d'origine d'une E46 320d (SW 1037361858) :
 *
 *   0x08000..0x0803F   valide
 *   0x08180..0x1FFF7   valide
 *   0x20000..0x5FFFB   invalide — la lecture s'arrête à 0x30000 (FF après)
 *   0xE0180..0xFEFFB   valide — la calibration, porte « /EDC16C31/ »
 *
 * On ne corrige QUE la région de calibration (celle qui porte la chaîne
 * famille) : c'est la seule qu'une modification de maps touche, et la
 * région programme n'est pas lue en entier — la « corriger » écrirait une
 * somme fausse dans une zone que l'outil de flash ne reprogramme pas.
 *
 * ⚠ La correction est la même opération que sur VAG (absorber l'écart dans
 * le dernier dword), validée sur l'invariant du fichier d'origine. Elle
 * n'a pas encore été comparée à un fichier corrigé par un outil connu :
 * les deux tunes du corpus ont été enregistrés sans correction (l'outil de
 * flash la fait à l'écriture).
 */

import { ChecksumResult, type ChecksumInfo } from './edc15p-checksum';

const MAGIC = [0xfa, 0xde, 0xca, 0xfe, 0xca, 0xfe, 0xaf, 0xfe];
const MEMORY_BASE = 0x800000;
const DUMP_SIZE = 0x100000;
const TARGET_SUM = 0xd01fe500;
const FAMILY_TOKEN = '/EDC16C31/';

export interface Edc16c31Region {
  /** Offset fichier du premier octet couvert */
  start: number;
  /** Offset fichier du dernier octet couvert (inclus) */
  end: number;
  /** Offset fichier du dword de checksum (end + 1 - 4) */
  checksumOffset: number;
  /** Région de calibration (porte la chaîne famille) : la seule corrigée */
  calibration: boolean;
}

function readU32BE(data: ArrayLike<number>, offset: number): number {
  return (
    (data[offset] * 0x1000000 +
      (data[offset + 1] << 16) +
      (data[offset + 2] << 8) +
      data[offset + 3]) >>>
    0
  );
}

function writeU32BE(data: number[], offset: number, value: number): void {
  data[offset] = (value >>> 24) & 0xff;
  data[offset + 1] = (value >>> 16) & 0xff;
  data[offset + 2] = (value >>> 8) & 0xff;
  data[offset + 3] = value & 0xff;
}

function indexOfAscii(data: ArrayLike<number>, text: string): number {
  const codes = Array.from(text, (c) => c.charCodeAt(0));
  outer: for (let i = 0; i + codes.length <= data.length; i++) {
    for (let k = 0; k < codes.length; k++) {
      if (data[i + k] !== codes[k]) continue outer;
    }
    return i;
  }
  return -1;
}

/** Régions protégées d'une lecture 1 Mo mappée à 0x800000. */
export function findEdc16c31Regions(fileData: ArrayLike<number>): Edc16c31Region[] {
  if (fileData.length !== DUMP_SIZE) return [];
  const token = indexOfAscii(fileData, FAMILY_TOKEN);
  const regions: Edc16c31Region[] = [];
  for (let i = 8; i + MAGIC.length <= fileData.length; i++) {
    if (fileData[i] !== MAGIC[0]) continue;
    let matched = true;
    for (let k = 1; k < MAGIC.length; k++) {
      if (fileData[i + k] !== MAGIC[k]) {
        matched = false;
        break;
      }
    }
    if (!matched) continue;
    const startAddr = readU32BE(fileData, i - 8);
    const endAddr = readU32BE(fileData, i - 4);
    if (startAddr < MEMORY_BASE || endAddr <= startAddr) continue;
    const start = startAddr - MEMORY_BASE;
    const end = endAddr - MEMORY_BASE;
    if (end >= fileData.length) continue;
    const length = end + 1 - start;
    if (length % 4 !== 0 || length < 64) continue;
    regions.push({
      start,
      end,
      checksumOffset: end + 1 - 4,
      calibration: token >= start && token <= end,
    });
  }
  return regions;
}

function sumRegion(data: ArrayLike<number>, start: number, end: number): number {
  let sum = 0;
  for (let i = start; i + 3 <= end; i += 4) {
    sum = (sum + readU32BE(data, i)) >>> 0;
  }
  return sum;
}

export interface Edc16c31RegionStatus extends Edc16c31Region {
  sum: number;
  stored: number;
  expected: number;
  valid: boolean;
}

export function analyzeEDC16C31Checksum(fileData: ArrayLike<number>): Edc16c31RegionStatus[] {
  return findEdc16c31Regions(fileData).map((region) => {
    const sum = sumRegion(fileData, region.start, region.end);
    const stored = readU32BE(fileData, region.checksumOffset);
    const expected = (stored + (TARGET_SUM - sum)) >>> 0;
    return { ...region, sum, stored, expected, valid: sum === TARGET_SUM };
  });
}

/**
 * Corrige le checksum de la région de calibration. Retourne `null` si le
 * fichier n'a pas de région de calibration reconnaissable (rien à faire,
 * l'appelant l'affiche comme « non supporté »). Les autres régions ne sont
 * jamais écrites et n'entrent pas dans le résultat.
 */
export function correctEDC16C31Checksum(
  fileData: number[]
): { info: ChecksumInfo; correctedData: number[] } | null {
  const cal = analyzeEDC16C31Checksum(fileData).filter((s) => s.calibration);
  if (cal.length !== 1) return null;
  const status = cal[0];
  const correctedData = [...fileData];
  if (!status.valid) {
    writeU32BE(correctedData, status.checksumOffset, status.expected);
  }
  return {
    correctedData,
    info: {
      result: status.valid ? ChecksumResult.ChecksumOK : ChecksumResult.ChecksumFail,
      found: 1,
      fixed: status.valid ? 0 : 1,
      matched: status.valid ? 1 : 0,
      variant: 'edc16',
    },
  };
}

export function verifyEDC16C31Checksum(fileData: ArrayLike<number>): ChecksumInfo | null {
  const cal = analyzeEDC16C31Checksum(fileData).filter((s) => s.calibration);
  if (cal.length !== 1) return null;
  const valid = cal[0].valid;
  return {
    result: valid ? ChecksumResult.ChecksumOK : ChecksumResult.ChecksumFail,
    found: 1,
    fixed: 0,
    matched: valid ? 1 : 0,
    variant: 'edc16',
  };
}
