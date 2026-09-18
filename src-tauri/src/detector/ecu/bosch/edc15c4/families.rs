//! Classification of the self-describing records (see layout.rs) into the
//! map families of the EDC15C4.
//!
//! Every rule reads the RECORD: grid, axis value ranges, data value ranges
//! and, for families that come in a fixed sequence, the neighbouring
//! records in file order. No rule reads an address, and none trusts an
//! axis id alone (an id is the RAM address of the input variable, and it
//! moves between builds: 30 of the 201 named records of the two reference
//! A2Ls carry a different id on the other build).
//!
//! The names come from the Bosch A2L of the reference project (P079.VB4):
//! the A2L identifier is quoted in each description, so a reader can go
//! back to the damos. The sequences (which of three identical grids is the
//! "dynamic" smoke limiter, which torque limiter is "raised") are the file
//! order of both reference builds and of the 525d read.

use super::layout::{Axis, Record};
use super::spec::{self, MapSpec};

/// Family of an inline record, with the spec used to display it.
#[derive(Debug, Clone)]
pub struct Classified {
    /// Index into the record list.
    pub index: usize,
    pub spec: MapSpec,
}

/// Rule thresholds that widen on a tuned file. Axis rules never widen.
#[derive(Debug, Clone, Copy)]
struct Limits {
    tuned: bool,
}

impl Limits {
    fn pick(&self, stock: u16, tuned: u16) -> u16 {
        if self.tuned { tuned } else { stock }
    }
}

// ----------------------------- axis shapes --------------------------------

/// Engine speed: 8..20 points from idle (or 0) up to 3500..6000 rpm.
fn is_rpm(a: &Axis) -> bool {
    (8..=20).contains(&a.len()) && a.first() <= 1100 && (3500..=6000).contains(&a.last())
}

/// Quantity (mm³/st ×100): from 0 up to 30..100 mm³.
fn is_iq(a: &Axis) -> bool {
    a.first() == 0 && (3000..=10_000).contains(&a.last())
}

/// Air mass (mg/st ×10): from 100+ mg up to 600..1200 mg.
fn is_air_mass(a: &Axis) -> bool {
    a.first() >= 1000 && (6000..=12_000).contains(&a.last())
}

/// Pedal (% ×100): from ~0 to 90..100 %.
fn is_pedal(a: &Axis) -> bool {
    a.first() < 500 && (9000..=10_000).contains(&a.last())
}

/// Temperature (0.1 K): -73..+167 degC.
fn is_temperature(a: &Axis) -> bool {
    a.values.iter().all(|&v| (2000..=4400).contains(&v))
}

/// Atmospheric pressure (hPa): 600..1100.
fn is_atmospheric(a: &Axis) -> bool {
    a.values.iter().all(|&v| (600..=1100).contains(&v))
}

/// Rail pressure (0.1 bar): 100..200 bar first, 1200..1600 bar last.
fn is_rail(a: &Axis) -> bool {
    (10..=20).contains(&a.len()) && (1000..=2000).contains(&a.first()) && (12_000..=16_000).contains(&a.last())
}

/// Cranking speed: 100..400 rpm first, 800..1500 rpm last.
fn is_cranking_rpm(a: &Axis) -> bool {
    (6..=10).contains(&a.len()) && (100..=400).contains(&a.first()) && (800..=1500).contains(&a.last())
}

fn second(r: &Record) -> Option<&Axis> {
    r.second.as_ref()
}

fn max_signed_abs(r: &Record) -> i32 {
    r.data.iter().map(|&v| (v as i16 as i32).abs()).max().unwrap_or(0)
}

// ------------------------------ 2D rules ----------------------------------

fn is_injector_duration(r: &Record, l: Limits) -> bool {
    let Some(iq) = second(r) else { return false };
    if !is_rail(&r.first) || !(20..=40).contains(&iq.len()) || !is_iq(iq) {
        return false;
    }
    if r.max() > l.pick(6000, 10_000) {
        return false;
    }
    (0..r.rows()).all(|i| {
        let row = r.row(i);
        row[0] == 0 && row.windows(2).all(|w| w[0] <= w[1])
    })
}

fn is_boost_target(r: &Record, l: Limits) -> bool {
    let Some(iq) = second(r) else { return false };
    if !is_rpm(&r.first) || !(6..=16).contains(&iq.len()) || !is_iq(iq) {
        return false;
    }
    if r.min() < 800 || r.max() > l.pick(3200, 4500) {
        return false;
    }
    (850..=1150).contains(&r.row(0)[0])
}

fn is_duty_base(r: &Record, _l: Limits) -> bool {
    let Some(iq) = second(r) else { return false };
    if !is_rpm(&r.first) || !(6..=16).contains(&iq.len()) || !is_iq(iq) {
        return false;
    }
    r.max() <= 10_000 && r.row(0)[0] >= 6000 && r.row(r.rows() - 1)[0] <= 5000
}

/// The 2 × 2 sport twin that immediately follows an eco map: same axis
/// shapes collapsed to two points, same physical window.
fn is_sport_twin(r: &Record, lo: u16, hi: u16) -> bool {
    let Some(iq) = second(r) else { return false };
    r.rows() == 2
        && iq.len() == 2
        && r.first.first() <= 1100
        && (3500..=6000).contains(&r.first.last())
        && iq.first() == 0
        && (3000..=10_000).contains(&iq.last())
        && r.min() >= lo
        && r.max() <= hi
}

fn is_duty_limit(r: &Record) -> bool {
    let Some(iq) = second(r) else { return false };
    is_rpm(&r.first) && (3..=6).contains(&iq.len()) && is_iq(iq) && r.max() <= 10_000
}

fn is_smoke_limiter(r: &Record, l: Limits) -> bool {
    let Some(air) = second(r) else { return false };
    is_rpm(&r.first)
        && r.rows() >= 12
        && (12..=20).contains(&air.len())
        && is_air_mass(air)
        && r.max() <= l.pick(10_000, 12_000)
        && r.max() > 0
}

fn is_smoke_correction(r: &Record) -> bool {
    let Some(air) = second(r) else { return false };
    is_rpm(&r.first)
        && (8..=12).contains(&r.rows())
        && (8..=12).contains(&air.len())
        && is_air_mass(air)
        && max_signed_abs(r) <= 3000
}

fn is_driver_wish(r: &Record, l: Limits) -> bool {
    let Some(pedal) = second(r) else { return false };
    is_rpm(&r.first)
        && (10..=16).contains(&r.rows())
        && (6..=10).contains(&pedal.len())
        && is_pedal(pedal)
        && r.max() <= l.pick(10_000, 12_000)
        && r.max() > 0
}

/// Rows: engine speed from 1750 rpm (the turbo only matters under load).
fn is_turbo_protection(r: &Record, l: Limits) -> bool {
    let Some(atm) = second(r) else { return false };
    let rpm = &r.first;
    (8..=12).contains(&rpm.len())
        && (1000..=2500).contains(&rpm.first())
        && (3500..=6000).contains(&rpm.last())
        && (6..=12).contains(&atm.len())
        && is_atmospheric(atm)
        && r.max() <= l.pick(10_000, 12_000)
}

/// Rows: engine speed up to 3000 rpm (the raise only applies at low speed).
fn is_rpm_by_coolant(r: &Record, l: Limits) -> bool {
    let Some(t) = second(r) else { return false };
    let rpm = &r.first;
    (6..=10).contains(&rpm.len())
        && rpm.first() <= 1100
        && (2500..=6000).contains(&rpm.last())
        && (6..=10).contains(&t.len())
        && is_temperature(t)
        && r.max() <= l.pick(10_000, 12_000)
}

fn is_start_quantity(r: &Record, l: Limits) -> bool {
    let Some(t) = second(r) else { return false };
    is_cranking_rpm(&r.first) && (8..=12).contains(&t.len()) && is_temperature(t) && r.max() <= l.pick(10_000, 12_000)
}

/// Rows: engine speed over the EGR window only (700..2500-2700 rpm).
fn is_egr_target(r: &Record) -> bool {
    let Some(iq) = second(r) else { return false };
    let rpm = &r.first;
    (10..=16).contains(&rpm.len())
        && (500..=1100).contains(&rpm.first())
        && (2000..=3500).contains(&rpm.last())
        && (12..=16).contains(&iq.len())
        && is_iq(iq)
        && r.min() >= 1000
        && r.max() <= 15_000
}

/// Rows = quantity, cols = engine speed; the rpm axis of this map stops
/// at 3000 rpm on the reference builds (EGR is closed above).
fn is_egr_duty(r: &Record) -> bool {
    let Some(rpm) = second(r) else { return false };
    (6..=10).contains(&r.rows())
        && is_iq(&r.first)
        && (6..=10).contains(&rpm.len())
        && rpm.first() <= 1100
        && (2500..=3500).contains(&rpm.last())
        && r.max() <= 10_000
}

// ------------------------------ 1D rules ----------------------------------

fn is_torque_curve(r: &Record, points: usize, l: Limits) -> bool {
    !r.is_2d() && r.rows() == points && is_rpm(&r.first) && r.max() <= l.pick(10_000, 12_000)
}

// ------------------------------- driver -----------------------------------

/// Classify every inline record. Sequence-dependent families are only
/// emitted when the sequence is complete (six durations, three smoke
/// limiters, three driver wish maps, three torque limiter curves in a
/// row): a partial set is a build the rules do not understand.
pub fn classify(records: &[Record], tuned: bool) -> Vec<Classified> {
    let l = Limits { tuned };
    let mut out: Vec<Classified> = Vec::new();
    let mut taken = vec![false; records.len()];
    let mut push = |taken: &mut Vec<bool>, index: usize, spec: MapSpec| {
        if !taken[index] {
            taken[index] = true;
            out.push(Classified { index, spec });
        }
    };

    // --- sequences -------------------------------------------------------
    let durations: Vec<usize> = (0..records.len()).filter(|&i| is_injector_duration(&records[i], l)).collect();
    if durations.len() == 6 {
        for (k, &i) in durations.iter().enumerate() {
            push(&mut taken, i, spec::duration(k));
        }
    } else if !durations.is_empty() {
        log::warn!("EDC15C4: {} injector duration maps instead of 6, none reported", durations.len());
    }

    let smokes: Vec<usize> = (0..records.len()).filter(|&i| is_smoke_limiter(&records[i], l)).collect();
    if smokes.len() == 3 {
        push(&mut taken, smokes[0], spec::SMOKE_DYNAMIC);
        push(&mut taken, smokes[1], spec::SMOKE_MAIN);
        push(&mut taken, smokes[2], spec::SMOKE_LOW_RANGE);
        // The full-load raise by coolant temperature sits between the
        // dynamic smoke limiter and the two corrections; the friction map
        // (same grid, same axes) sits much earlier in the block.
        for i in smokes[0] + 1..smokes[1] {
            if is_rpm_by_coolant(&records[i], l) {
                push(&mut taken, i, spec::FULL_LOAD_COOLANT);
                break;
            }
        }
        let corrections: Vec<usize> =
            (smokes[0] + 1..smokes[1]).filter(|&i| is_smoke_correction(&records[i])).collect();
        if corrections.len() == 2 {
            push(&mut taken, corrections[0], spec::SMOKE_CORR_ATM);
            push(&mut taken, corrections[1], spec::SMOKE_CORR_AIR_TEMP);
        }
    } else if !smokes.is_empty() {
        log::warn!("EDC15C4: {} smoke limiter maps instead of 3, none reported", smokes.len());
    }

    let wishes: Vec<usize> = (0..records.len()).filter(|&i| is_driver_wish(&records[i], l)).collect();
    if wishes.len() == 3 {
        for (k, &i) in wishes.iter().enumerate() {
            push(&mut taken, i, spec::driver_wish(k));
        }
    } else if !wishes.is_empty() {
        log::warn!("EDC15C4: {} driver wish maps instead of 3, none reported", wishes.len());
    }

    // Three consecutive 19-point limiter curves (pull-away, raised,
    // normal), then the 16-point low range curve right after.
    for i in 0..records.len().saturating_sub(2) {
        if (0..3).all(|k| is_torque_curve(&records[i + k], 19, l)) {
            for k in 0..3 {
                push(&mut taken, i + k, spec::torque_limiter(k));
            }
            if i + 3 < records.len() && is_torque_curve(&records[i + 3], 16, l) {
                push(&mut taken, i + 3, spec::TORQUE_LIMITER_LOW_RANGE);
            }
            break;
        }
    }

    // --- pairs -----------------------------------------------------------
    // Eco map, then its 2 × 2 sport twin immediately after (a 2-point
    // curve may sit between them on some builds - look two records ahead).
    let mut boost_eco = None;
    for i in 0..records.len() {
        if is_boost_target(&records[i], l) && boost_eco.is_none() {
            boost_eco = Some(i);
            push(&mut taken, i, spec::BOOST_TARGET_ECO);
            for j in i + 1..(i + 3).min(records.len()) {
                if is_sport_twin(&records[j], 800, l.pick(3200, 4500)) {
                    push(&mut taken, j, spec::BOOST_TARGET_SPORT);
                    break;
                }
            }
        }
    }
    let mut duty_eco = None;
    for i in 0..records.len() {
        if !taken[i] && is_duty_base(&records[i], l) && duty_eco.is_none() {
            duty_eco = Some(i);
            push(&mut taken, i, spec::DUTY_BASE_ECO);
            for j in i + 1..(i + 3).min(records.len()) {
                if is_sport_twin(&records[j], 0, 10_000) {
                    push(&mut taken, j, spec::DUTY_BASE_SPORT);
                    break;
                }
            }
        }
    }
    // Duty limits: two consecutive rpm × IQ(3..6) maps, max then min.
    for i in 0..records.len().saturating_sub(1) {
        if !taken[i] && is_duty_limit(&records[i]) && is_duty_limit(&records[i + 1]) {
            let (a, b) = (&records[i], &records[i + 1]);
            if a.max() > b.max() && a.rows() == b.rows() && a.cols() == b.cols() {
                push(&mut taken, i, spec::DUTY_LIMIT_MAX);
                push(&mut taken, i + 1, spec::DUTY_LIMIT_MIN);
                break;
            }
        }
    }

    // --- singles ---------------------------------------------------------
    for i in 0..records.len() {
        if taken[i] {
            continue;
        }
        let r = &records[i];
        if is_turbo_protection(r, l) {
            push(&mut taken, i, spec::TURBO_PROTECTION);
        } else if is_start_quantity(r, l) {
            push(&mut taken, i, spec::START_QUANTITY);
        } else if is_egr_target(r) {
            push(&mut taken, i, spec::EGR_TARGET);
        } else if is_egr_duty(r)
            && records.get(i + 1).map_or(false, |n| !n.is_2d() && (24..=40).contains(&n.rows()))
        {
            // The post-injection base map has the same shape; the EGR duty
            // map is the one followed by the 32-point MAF linearisation.
            push(&mut taken, i, spec::EGR_DUTY);
        }
    }

    // Unique families: if a rule fired twice the rule is wrong for this
    // build, drop both rather than guess.
    let mut names: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for c in &out {
        *names.entry(c.spec.name).or_default() += 1;
    }
    out.retain(|c| {
        let dup = names[c.spec.name] > 1;
        if dup {
            log::warn!("EDC15C4: '{}' matched {} records, dropped", c.spec.name, names[c.spec.name]);
        }
        !dup
    });
    out.sort_by_key(|c| c.index);
    out
}
