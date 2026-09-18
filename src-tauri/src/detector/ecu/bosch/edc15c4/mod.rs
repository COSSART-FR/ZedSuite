// Bosch EDC15C4 detector (BMW DDE 4.0 - M57 / M47 common rail, 512 KB).
// PORTING SKELETON, calibrated on ONE dump - see docs/PORTING-EDC15C4.md.
//
// What is reusable from the VAG EDC15 detectors and what is not
// ---------------------------------------------------------------
// REUSABLE (implemented in layout.rs, family-generic):
//   - the C167 little-endian u16 decoding;
//   - the self-describing record format [id][n][axis] [id][m][axis] [data],
//     the same one the EDC15P/EDC15VM modules walk. The block is read, not
//     guessed: a record is consumed whole, so a data block can never pass
//     for a header. No address is hardcoded.
//
// NOT REUSABLE (and deliberately not shared with the VAG modules):
//   - the axis id tables. On this family C016 is an rpm axis on most maps
//     but also carries 14..23 or 205..1100 on others, so an id names a
//     STORAGE family, not a physical quantity. Families are recognised by
//     grid + axis value ranges + data value ranges.
//   - the VAG pattern list (complete_patterns.rs): the grids, factors and
//     names are VAG software layouts.
//   - the v4.1 checksum: the block is signed "V2.0", and the frontend
//     routes EDC15C4 AWAY from the VAG checksum corrector (see
//     src/lib/ecu-family.ts). A wrong checksum algorithm silently writes
//     garbage at fixed addresses, which is the worst outcome of this engine.
//
// Safety contract of this module
// ------------------------------
// `detect()` only emits families whose `calibrated()` is true: two families
// whose physics is unambiguous on the reference dump (the six injector
// duration maps, the boost target map). Everything else the walk finds is
// exposed through `inventory()` for bench work, tagged with a hypothesis,
// and NEVER shown to the user until a reference (damos / WinOLS pack /
// second dump) confirms it. Per CONTRIBUTING.md: "fewer maps that are right
// rather than more maps that are almost right".

pub mod layout;

pub use layout::{parse_records, Axis, Edc15c4Layout, Record};

use crate::models::{DataType, DetectedMap, MapCategory, MapDimensions};

// ============================== FAMILIES ==============================

/// Map families of the EDC15C4 calibration block.
///
/// `calibrated()` is the gate: a family stays out of `detect()` until its
/// grid, factors and physical ranges have been confirmed on a real file
/// against a reference. Flip it with a dump AND a reference in front of you.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    /// Injector energising time by rail pressure (rows) and IQ (cols),
    /// six maps in a row. CALIBRATED: rail axis 0.1 bar (119..1450),
    /// IQ axis 0.01 mg/st (0..70), data in µs, 0 at IQ 0, monotonic along
    /// IQ, decreasing with rail pressure at a given IQ.
    InjectorDuration,
    /// Boost pressure set point by rpm (rows) and IQ (cols), absolute mbar.
    /// CALIBRATED: 990..1030 mbar at idle/no load (atmospheric), 2200 mbar
    /// at full load on the reference 525d (1.2 bar relative).
    BoostTarget,
    /// HYPOTHESIS: rpm × IQ map with 85 % at idle falling to 27 % at
    /// 4600 rpm - the shape of a VNT actuator base duty (0.01 %), but an
    /// EGR duty has a similar shape. Needs a reference.
    ActuatorDutyByRpmIq,
    /// HYPOTHESIS: 16 × 16 rpm × (2000..8500) map with data saturating at
    /// a per-rpm ceiling (5013 at 650 rpm, 3554 at 4900 rpm): looks like a
    /// torque request → IQ conversion with a limiter folded in. Two
    /// identical copies plus a third variant on the reference file.
    TorqueToIq,
    /// HYPOTHESIS: 1D curve on a 10-bit ADC axis (0..1023) producing
    /// tenths of kelvin (2331..4131): NTC sensor linearisation. Correct
    /// but of no tuning value - not exposed.
    SensorLinearisation,
    /// 1D or 2D record the walk found but no rule names. Kept in the
    /// inventory so the bench can list it.
    Unknown,
}

impl Family {
    pub const fn calibrated(self) -> bool {
        matches!(self, Family::InjectorDuration | Family::BoostTarget)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Family::InjectorDuration => "Injector duration",
            Family::BoostTarget => "Boost target map",
            Family::ActuatorDutyByRpmIq => "Actuator duty by rpm/IQ (hypothesis)",
            Family::TorqueToIq => "Torque to IQ (hypothesis)",
            Family::SensorLinearisation => "Sensor linearisation (hypothesis)",
            Family::Unknown => "Unclassified record",
        }
    }
}

// ============================== DETECTOR ==============================

pub struct EDC15C4Detector {
    tuned: bool,
}

impl Default for EDC15C4Detector {
    fn default() -> Self {
        Self::new()
    }
}

impl EDC15C4Detector {
    pub fn new() -> Self {
        Self { tuned: false }
    }

    /// Tuned mode widens the data windows (a remapped duration or boost
    /// map exceeds the stock ceilings) without touching the axis rules.
    pub fn new_tuned() -> Self {
        Self { tuned: true }
    }

    /// Every record of the calibration block, with the family it was
    /// recognised as. Empty when the file carries no V2.0 block.
    pub fn inventory(&self, data: &[u8]) -> Vec<(Record, Family)> {
        let Some(layout) = Edc15c4Layout::detect(data) else {
            return Vec::new();
        };
        parse_records(data, layout.block_start, layout.block_end)
            .into_iter()
            .map(|r| {
                let f = self.classify(&r);
                (r, f)
            })
            .collect()
    }

    /// Calibrated families only. Never falls back to a VAG detector.
    pub fn detect(&self, data: &[u8]) -> Vec<DetectedMap> {
        if data.len() != layout::DUMP_SIZE {
            return Vec::new();
        }
        let Some(layout) = Edc15c4Layout::detect(data) else {
            return Vec::new();
        };
        let mut maps = Vec::new();
        let mut duration_index = 0usize;
        for (rec, family) in self.inventory(data) {
            if !family.calibrated() {
                continue;
            }
            let map = match family {
                Family::InjectorDuration => {
                    let m = self.duration_map(&rec, duration_index, &layout);
                    duration_index += 1;
                    m
                }
                Family::BoostTarget => self.boost_target_map(&rec, &layout),
                _ => continue,
            };
            maps.push(map);
        }
        // Six durations is the invariant of the family (one per injector
        // slot, as on every Bosch EDC15). A file with another count is a
        // file the rules do not understand: report none rather than a
        // partial set the user would take for complete.
        let durations = maps.iter().filter(|m| m.subcategory.as_deref() == Some("duration")).count();
        if durations != 0 && durations != 6 {
            log::warn!("EDC15C4: {} injector duration maps instead of 6, dropping them", durations);
            maps.retain(|m| m.subcategory.as_deref() != Some("duration"));
        }
        maps
    }

    // ------------------------------------------------------------------
    // Classification rules. Each rule reads the RECORD (grid, axis values,
    // data values), never an address and never an id alone.
    // ------------------------------------------------------------------

    pub fn classify(&self, r: &Record) -> Family {
        if self.is_injector_duration(r) {
            return Family::InjectorDuration;
        }
        if self.is_boost_target(r) {
            return Family::BoostTarget;
        }
        if self.is_actuator_duty(r) {
            return Family::ActuatorDutyByRpmIq;
        }
        if self.is_torque_to_iq(r) {
            return Family::TorqueToIq;
        }
        if self.is_sensor_linearisation(r) {
            return Family::SensorLinearisation;
        }
        Family::Unknown
    }

    fn data_ceiling(&self, stock: u16, tuned: u16) -> u16 {
        if self.tuned { tuned } else { stock }
    }

    /// rpm-shaped axis: 8..20 points, starts at or below 1100, ends
    /// between 3500 and 6000 rpm.
    fn is_rpm_axis(a: &Axis) -> bool {
        (8..=20).contains(&a.len()) && a.first() <= 1100 && (3500..=6000).contains(&a.last())
    }

    /// IQ-shaped axis in 0.01 mg/st: starts at 0, ends between 30 and
    /// 100 mg/st.
    fn is_iq_axis(a: &Axis) -> bool {
        a.first() == 0 && (3000..=10_000).contains(&a.last())
    }

    fn is_injector_duration(&self, r: &Record) -> bool {
        let Some(iq) = &r.second else { return false };
        let rail = &r.first;
        // Rail pressure axis, 0.1 bar: 10..20 points, 100..200 bar first,
        // 1200..1600 bar last.
        if !(10..=20).contains(&rail.len())
            || !(1000..=2000).contains(&rail.first())
            || !(12_000..=16_000).contains(&rail.last())
        {
            return false;
        }
        if !(20..=40).contains(&iq.len()) || !Self::is_iq_axis(iq) {
            return false;
        }
        let ceiling = self.data_ceiling(6000, 10_000);
        if r.max() > ceiling {
            return false;
        }
        // IQ 0 -> 0 µs on every rail row, and non-decreasing along IQ.
        (0..r.rows()).all(|i| {
            let row = r.row(i);
            row[0] == 0 && row.windows(2).all(|w| w[0] <= w[1])
        })
    }

    fn is_boost_target(&self, r: &Record) -> bool {
        let Some(iq) = &r.second else { return false };
        let rpm = &r.first;
        if !Self::is_rpm_axis(rpm) || !(6..=16).contains(&iq.len()) || !Self::is_iq_axis(iq) {
            return false;
        }
        let ceiling = self.data_ceiling(3200, 4500);
        if r.min() < 800 || r.max() > ceiling {
            return false;
        }
        // No load at the lowest rpm = atmospheric pressure (absolute mbar).
        let idle_no_load = r.row(0)[0];
        (850..=1150).contains(&idle_no_load)
    }

    fn is_actuator_duty(&self, r: &Record) -> bool {
        let Some(iq) = &r.second else { return false };
        let rpm = &r.first;
        if !Self::is_rpm_axis(rpm) || !(6..=16).contains(&iq.len()) || !Self::is_iq_axis(iq) {
            return false;
        }
        // Percent in 0.01 %: never above 100 %, high at low rpm, low at
        // high rpm (vanes close to spool, open at speed).
        r.max() <= 10_000 && r.row(0)[0] >= 7000 && r.row(r.rows() - 1)[0] <= 4000
    }

    fn is_torque_to_iq(&self, r: &Record) -> bool {
        let Some(req) = &r.second else { return false };
        let rpm = &r.first;
        if !Self::is_rpm_axis(rpm) || !(12..=20).contains(&req.len()) {
            return false;
        }
        if req.first() < 1000 || req.last() > 12_000 || r.max() > 10_000 {
            return false;
        }
        // Rows rise with the request (more request, more fuel; a few
        // one-bit dips are tolerated, the reference has 1914 -> 1911) and
        // the last column of at least half the rows is a plateau (the
        // limiter folded in).
        if r.max() == 0 {
            return false;
        }
        let pairs: usize = (0..r.rows()).map(|i| r.row(i).len() - 1).sum();
        let rising: usize = (0..r.rows())
            .map(|i| r.row(i).windows(2).filter(|w| w[0] <= w[1]).count())
            .sum();
        let rows_ok = rising * 10 >= pairs * 9;
        let plateaus = (0..r.rows())
            .filter(|&i| {
                let row = r.row(i);
                let c = row.len();
                row[c - 1] == row[c - 2]
            })
            .count();
        rows_ok && plateaus * 2 >= r.rows()
    }

    fn is_sensor_linearisation(&self, r: &Record) -> bool {
        if r.is_2d() {
            return false;
        }
        let adc = &r.first;
        // 10-bit converter axis producing tenths of kelvin (-50..+140 degC).
        adc.last() <= 1023
            && adc.len() >= 4
            && r.data.iter().all(|&v| (2231..=4131).contains(&v))
            && r.data.windows(2).all(|w| w[0] >= w[1])
            && r.data[0] > r.data[r.data.len() - 1]
    }

    // ------------------------------------------------------------------
    // DetectedMap builders. rows = first axis (Y), cols = second axis (X).
    // ------------------------------------------------------------------

    fn base_map(&self, r: &Record, layout: &Edc15c4Layout) -> DetectedMap {
        let mut m = DetectedMap::new(
            r.data_addr as u32,
            r.data_size(),
            MapDimensions::TwoDimensional { rows: r.rows(), cols: r.cols() },
            DataType::UInt16,
        );
        m.is_little_endian = Some(true);
        m.y_axis_address = Some(r.first.values_addr as u32);
        m.x_axis_address = r.second.as_ref().map(|a| a.values_addr as u32);
        m.codeblock_id = Some(1);
        m.codeblock_start_address = Some(layout.block_start as u32);
        m.codeblock_end_address = Some(layout.block_end as u32);
        m
    }

    fn duration_map(&self, r: &Record, index: usize, layout: &Edc15c4Layout) -> DetectedMap {
        let mut m = self.base_map(r, layout);
        m.id = format!("edc15c4_dur_{:02}_{:06X}", index, r.data_addr);
        m.name = Some(format!("Injector duration {:02}", index));
        m.category = Some(MapCategory::InjectionSystem.display_name().to_string());
        m.subcategory = Some("duration".to_string());
        m.unit = Some("µs".to_string());
        m.correction_factor = Some(1.0);
        m.offset = Some(0.0);
        m.y_label = Some("Rail pressure (bar)".to_string());
        m.y_axis_correction = Some(0.1);
        m.y_axis_offset = Some(0.0);
        m.x_label = Some("IQ (mg/st)".to_string());
        m.x_axis_correction = Some(0.01);
        m.x_axis_offset = Some(0.0);
        m.description = Some(
            "Injector energising time for a requested quantity at a given rail pressure | X: IQ (mg/st) | Y: Rail pressure (bar)".to_string(),
        );
        m.confidence = 0.90;
        m
    }

    fn boost_target_map(&self, r: &Record, layout: &Edc15c4Layout) -> DetectedMap {
        let mut m = self.base_map(r, layout);
        m.id = format!("edc15c4_boost_{:06X}", r.data_addr);
        m.name = Some("Boost target map".to_string());
        m.category = Some(MapCategory::TurboBoostPressure.display_name().to_string());
        m.subcategory = Some("target".to_string());
        m.unit = Some("mbar".to_string());
        m.correction_factor = Some(1.0);
        m.offset = Some(0.0);
        m.y_label = Some("Engine speed (rpm)".to_string());
        m.y_axis_correction = Some(1.0);
        m.y_axis_offset = Some(0.0);
        m.x_label = Some("IQ (mg/st)".to_string());
        m.x_axis_correction = Some(0.01);
        m.x_axis_offset = Some(0.0);
        m.description = Some(
            "Absolute boost pressure set point by engine speed and injected quantity | X: IQ (mg/st) | Y: Engine speed (rpm)".to_string(),
        );
        m.confidence = 0.85;
        m
    }
}

// =============================== TESTS ================================

#[cfg(test)]
mod tests {
    use super::*;

    fn put16(buf: &mut Vec<u8>, v: u16) {
        buf.extend_from_slice(&v.to_le_bytes());
    }

    fn record_2d(id1: u16, ax1: &[u16], id2: u16, ax2: &[u16], z: &[u16]) -> Vec<u8> {
        let mut b = Vec::new();
        put16(&mut b, id1);
        put16(&mut b, ax1.len() as u16);
        ax1.iter().for_each(|&v| put16(&mut b, v));
        put16(&mut b, id2);
        put16(&mut b, ax2.len() as u16);
        ax2.iter().for_each(|&v| put16(&mut b, v));
        z.iter().for_each(|&v| put16(&mut b, v));
        b
    }

    /// A blank 512 KB dump carrying only the V2.0 block header.
    fn blank_dump() -> Vec<u8> {
        let mut data = vec![0xC3u8; layout::DUMP_SIZE];
        data[0x70000] = 0xF9;
        data[0x70001..0x70001 + 11].copy_from_slice(&layout::V20_SIGNATURE);
        data
    }

    const RAIL: [u16; 15] = [1190, 1200, 2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000, 10000, 11000, 12000, 13500, 14500];
    const IQ32: [u16; 32] = [
        0, 2, 50, 100, 150, 200, 270, 340, 400, 500, 600, 700, 800, 950, 1250, 1500, 1750, 2000, 2250,
        2500, 2750, 3000, 3250, 3500, 3750, 4000, 4250, 4500, 5000, 5500, 6000, 7000,
    ];

    fn duration_record() -> Vec<u8> {
        // Row r: 0 at IQ 0, then increasing, lower rows (higher rail) shorter.
        let mut z = Vec::new();
        for r in 0..RAIL.len() {
            for c in 0..IQ32.len() {
                let v = if c == 0 { 0 } else { (c as u16 * 120).saturating_sub(r as u16 * 40).max(20) };
                z.push(v.min(5000));
            }
        }
        record_2d(0xC032, &RAIL, 0xD900, &IQ32, &z)
    }

    const RPM16: [u16; 16] = [0, 850, 1008, 1250, 1500, 1750, 2000, 2250, 2500, 2750, 3003, 3507, 4000, 4200, 4408, 4600];
    const IQ10: [u16; 10] = [0, 1000, 1200, 1500, 2000, 2500, 3500, 4000, 4500, 5000];

    fn boost_record() -> Vec<u8> {
        let mut z = Vec::new();
        for r in 0..RPM16.len() {
            for c in 0..IQ10.len() {
                z.push(990 + (c as u16 * 120) + (r as u16 * 5));
            }
        }
        record_2d(0xC016, &RPM16, 0xC314, &IQ10, &z)
    }

    #[test]
    fn uncalibrated_families_are_never_emitted() {
        let d = EDC15C4Detector::new();
        for f in [Family::ActuatorDutyByRpmIq, Family::TorqueToIq, Family::SensorLinearisation, Family::Unknown] {
            assert!(!f.calibrated(), "{:?} must stay a hypothesis", f);
        }
        assert!(d.detect(&vec![0xC3u8; layout::DUMP_SIZE]).is_empty(), "no block, no maps");
    }

    #[test]
    fn six_durations_and_one_boost_target_on_a_synthetic_block() {
        let mut data = blank_dump();
        let mut off = 0x71800;
        for _ in 0..6 {
            let rec = duration_record();
            data[off..off + rec.len()].copy_from_slice(&rec);
            off += rec.len();
        }
        let boost = boost_record();
        data[off..off + boost.len()].copy_from_slice(&boost);

        let maps = EDC15C4Detector::new().detect(&data);
        assert_eq!(maps.len(), 7, "{:?}", maps.iter().map(|m| m.name.clone()).collect::<Vec<_>>());
        let names: Vec<String> = maps.iter().filter_map(|m| m.name.clone()).collect();
        assert_eq!(names[0], "Injector duration 00");
        assert_eq!(names[5], "Injector duration 05");
        assert_eq!(names[6], "Boost target map");

        // Axes: rows = rail (first axis), cols = IQ (second axis).
        let d0 = &maps[0];
        assert!(matches!(d0.dimensions, MapDimensions::TwoDimensional { rows: 15, cols: 32 }));
        assert_eq!(d0.y_axis_address, Some(0x71800 + 4));
        assert_eq!(d0.x_axis_address, Some(0x71800 + 4 + 30 + 4));
        assert_eq!(d0.address, 0x71800 + 4 + 30 + 4 + 64);
        assert_eq!(d0.is_little_endian, Some(true));
        assert_eq!(d0.y_axis_correction, Some(0.1));
        assert_eq!(d0.x_axis_correction, Some(0.01));
    }

    #[test]
    fn a_partial_duration_set_is_dropped_whole() {
        let mut data = blank_dump();
        let rec = duration_record();
        data[0x71800..0x71800 + rec.len()].copy_from_slice(&rec);
        let maps = EDC15C4Detector::new().detect(&data);
        assert!(maps.is_empty(), "one duration out of six must not be shown as the duration set");
    }

    #[test]
    fn a_boost_map_without_atmospheric_baseline_is_refused() {
        let mut data = blank_dump();
        let mut z = Vec::new();
        for _ in 0..RPM16.len() {
            for c in 0..IQ10.len() {
                z.push(1500 + c as u16 * 50);
            }
        }
        let rec = record_2d(0xC016, &RPM16, 0xC314, &IQ10, &z);
        data[0x71800..0x71800 + rec.len()].copy_from_slice(&rec);
        assert!(EDC15C4Detector::new().detect(&data).is_empty());
    }

    #[test]
    fn stock_ceiling_refuses_a_remapped_boost_map_unless_tuned() {
        let mut data = blank_dump();
        let mut z = Vec::new();
        for r in 0..RPM16.len() {
            for c in 0..IQ10.len() {
                z.push(if r == 0 && c == 0 { 1000 } else { 3600 });
            }
        }
        let rec = record_2d(0xC016, &RPM16, 0xC314, &IQ10, &z);
        data[0x71800..0x71800 + rec.len()].copy_from_slice(&rec);
        assert!(EDC15C4Detector::new().detect(&data).is_empty());
        assert_eq!(EDC15C4Detector::new_tuned().detect(&data).len(), 1);
    }

    #[test]
    fn wrong_size_is_refused() {
        let data = vec![0xC3u8; 0x100000];
        assert!(EDC15C4Detector::new().detect(&data).is_empty());
    }
}
