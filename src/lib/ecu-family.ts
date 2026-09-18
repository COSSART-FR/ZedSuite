/**
 * ECU family predicates shared by the frontend paths that key on a type
 * name substring.
 *
 * Several call sites route on `ecuType.includes("EDC15")` to reach the VAG
 * TDI logic (v4.1 checksum, EDC15P DTC tables, launch control). The Bosch
 * EDC15C4 (BMW DDE 4.0) shares the "EDC15" prefix but none of that: its
 * calibration block is signed "V2.0", its DTC table is elsewhere and its
 * maps are not the VAG ones. Applying the VAG checksum corrector to it would
 * write bogus sums at fixed VAG addresses. These predicates are the single
 * place that knows the difference.
 */

/** Bosch EDC15C4 - BMW DDE 4.0 (M57 / M47 common rail). */
export function isEdc15c4(ecuType: string | undefined): boolean {
  return (ecuType ?? "").toUpperCase() === "EDC15C4";
}

/**
 * VAG EDC15 family (EDC15P / EDC15V / EDC15VM / EDC15M / EDC15C): the
 * families that share the Bosch VAG TDI v4.1 checksum, DTC tables and
 * solutions. EDC15C4 is excluded on purpose.
 */
export function isVagEdc15(ecuType: string | undefined): boolean {
  const upper = (ecuType ?? "").toUpperCase();
  return upper.includes("EDC15") && !isEdc15c4(upper);
}
