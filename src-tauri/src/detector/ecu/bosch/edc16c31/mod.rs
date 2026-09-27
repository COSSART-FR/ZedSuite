// Bosch EDC16C31 detector (BMW DDE, 1 MB reads mapped at 0x800000).
// Calibrated on one software: E46 320d 150 hp (M47TU), Bosch SW
// 1037361858 - see docs/PORTING-EDC16C31.md.
//
// How it reads a file
// -------------------
// 1. layout.rs finds the calibration region through the Bosch EDC16
//    checksum descriptors (the region carrying "/EDC16C31/"), then scans it
//    for self-describing `Kf` maps [nx][ny][axis][axis][data] and, in the
//    gaps between them, `Kl` curves [n][axis][data]. Big-endian, the data
//    of a map is stored with the second axis as the fast index.
// 2. families.rs names the blocks from their grid, axis value ranges and
//    data ranges, and only when a family answers with its full set.
//
// Nothing is shared with the VAG EDC16 modules (axis-first map guessing,
// VAG checksum layout) nor with the EDC16CP31 module (whose templates are
// keyed on the Mercedes software).
//
// Safety contract
// ---------------
// `detect()` refuses any size other than 1 MB and any file without a
// checksum region carrying the family string. It never falls back to a
// VAG detector.

pub mod families;
pub mod layout;
pub mod spec;

pub use layout::{Axis, Block, ChecksumRegion};
pub use spec::MapSpec;

use crate::models::{DataType, DetectedMap, MapDimensions};

/// One block of the calibration region with the family it was recognised
/// as (None = unclassified). Used by the bench and the fixture tests.
#[derive(Debug, Clone)]
pub struct InventoryEntry {
    pub block: Block,
    pub spec: Option<MapSpec>,
}

pub struct EDC16C31Detector {
    tuned: bool,
}

impl Default for EDC16C31Detector {
    fn default() -> Self {
        Self::new()
    }
}

impl EDC16C31Detector {
    pub fn new() -> Self {
        Self { tuned: false }
    }

    /// Tuned mode widens the data ceilings (a remapped map exceeds the
    /// stock ones) without touching the axis rules.
    pub fn new_tuned() -> Self {
        Self { tuned: true }
    }

    fn blocks(&self, data: &[u8]) -> Option<(ChecksumRegion, Vec<Block>, Vec<Block>)> {
        if data.len() != layout::DUMP_SIZE {
            return None;
        }
        let region = layout::calibration_region(data)?;
        let hi = region.end + 1;
        let maps = layout::scan_maps(data, region.start, hi);
        let curves = layout::scan_curves(data, &maps, region.start, hi);
        Some((region, maps, curves))
    }

    /// Every block of the calibration region, classified, in address order.
    pub fn inventory(&self, data: &[u8]) -> Vec<InventoryEntry> {
        let Some((_, maps, curves)) = self.blocks(data) else {
            return Vec::new();
        };
        let classified = families::classify(&maps, &curves, self.tuned);
        let mut all: Vec<InventoryEntry> = maps
            .into_iter()
            .chain(curves)
            .map(|block| {
                let spec = classified.iter().find(|c| c.block.addr == block.addr).map(|c| c.spec);
                InventoryEntry { block, spec }
            })
            .collect();
        all.sort_by_key(|e| e.block.addr);
        all
    }

    /// The maps of the file. Never falls back to a VAG detector.
    pub fn detect(&self, data: &[u8]) -> Vec<DetectedMap> {
        let Some((region, maps, curves)) = self.blocks(data) else {
            return Vec::new();
        };
        let classified = families::classify(&maps, &curves, self.tuned);
        let mut out: Vec<DetectedMap> = classified.iter().map(|c| self.build(&c.spec, &c.block, &region)).collect();
        if let Some(addr) = self.find_svbl(data, &maps, &classified) {
            out.push(self.build_single(&spec::SVBL, addr, &region));
            out.sort_by_key(|m| m.address);
        }
        out
    }

    /// SVBL: the single value right before the block that follows the
    /// atmospheric boost limiter, itself followed by the minimum boost set
    /// point (0 on the stock reads). Same place on the two reference
    /// softwares (0xF9C60 on SW 1037361858, 0xFA268 on SW 1037361811 where
    /// a WinOLS project names it "Größter erlaubter Ladedrucksollwert"), and
    /// the 320d stage 1 raises it 2400 -> 2520 hPa.
    fn find_svbl(&self, data: &[u8], maps: &[Block], classified: &[families::Classified]) -> Option<usize> {
        let limiter = classified.iter().find(|c| c.spec.name == spec::BOOST_LIMITER_ATM.name)?;
        let next = maps.iter().find(|m| m.addr > limiter.block.addr)?;
        let at = next.addr.checked_sub(4)?;
        if at < limiter.block.end() {
            return None;
        }
        let value = i16::from_be_bytes([data[at], data[at + 1]]);
        let min = i16::from_be_bytes([data[at + 2], data[at + 3]]);
        let ceiling = if self.tuned { 4500 } else { 4000 };
        ((1500..=ceiling).contains(&value) && (0..value).contains(&min)).then_some(at)
    }

    fn build_single(&self, spec: &MapSpec, addr: usize, region: &ChecksumRegion) -> DetectedMap {
        let mut m = DetectedMap::new(addr as u32, 2, MapDimensions::OneDimensional { length: 1 }, DataType::UInt16);
        m.id = format!("edc16c31_{:06X}", addr);
        m.name = Some(spec.name.to_string());
        m.category = Some(spec.category.display_name().to_string());
        m.subcategory = Some(spec.subcategory.to_string());
        m.description = Some(spec.description.to_string());
        m.unit = Some(spec.unit.to_string());
        m.correction_factor = Some(spec.z_factor);
        m.offset = Some(0.0);
        m.confidence = spec.confidence;
        m.is_little_endian = Some(false);
        m.as_stored = Some(true);
        m.codeblock_id = Some(1);
        m.codeblock_start_address = Some(region.start as u32);
        m.codeblock_end_address = Some((region.end + 1) as u32);
        m
    }

    /// rows = first axis (Y), cols = second axis (X); big-endian.
    fn build(&self, spec: &MapSpec, b: &Block, region: &ChecksumRegion) -> DetectedMap {
        let (rows, cols) = (b.rows(), b.cols());
        let dims = if cols == 1 {
            MapDimensions::OneDimensional { length: rows }
        } else {
            MapDimensions::TwoDimensional { rows, cols }
        };
        let data_type = if spec.signed { DataType::Int16 } else { DataType::UInt16 };
        let mut m = DetectedMap::new(b.data_addr as u32, rows * cols * 2, dims, data_type);
        m.id = format!("edc16c31_{:06X}", b.data_addr);
        m.name = Some(spec.name.to_string());
        m.category = Some(spec.category.display_name().to_string());
        m.subcategory = Some(spec.subcategory.to_string());
        m.description = Some(spec.description.to_string());
        m.unit = Some(spec.unit.to_string());
        m.correction_factor = Some(spec.z_factor);
        m.offset = Some(0.0);
        m.confidence = spec.confidence;
        m.is_little_endian = Some(false);
        // Rows = first axis, cols = second axis, exactly as stored: keep the
        // frontend's VAG name-based transposition rules away from these maps.
        m.as_stored = Some(true);
        m.codeblock_id = Some(1);
        m.codeblock_start_address = Some(region.start as u32);
        m.codeblock_end_address = Some((region.end + 1) as u32);
        // A curve has its single axis along the rows; the app reads a 1D
        // map's axis from x_axis_address, so the row axis goes there.
        match (&b.second, spec.x) {
            (Some(second), Some(x)) => {
                m.y_axis_address = Some(b.first.values_addr as u32);
                m.y_label = Some(spec.y.label.to_string());
                m.y_axis_correction = Some(spec.y.factor);
                m.y_axis_offset = Some(spec.y.offset);
                m.x_axis_address = Some(second.values_addr as u32);
                m.x_label = Some(x.label.to_string());
                m.x_axis_correction = Some(x.factor);
                m.x_axis_offset = Some(x.offset);
            }
            _ => {
                m.x_axis_address = Some(b.first.values_addr as u32);
                m.x_label = Some(spec.y.label.to_string());
                m.x_axis_correction = Some(spec.y.factor);
                m.x_axis_offset = Some(spec.y.offset);
            }
        }
        m
    }
}

// =============================== TESTS ================================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const CAL_START: usize = 0xE0180;
    pub const CAL_END: usize = 0xFEFFB;

    /// A blank 1 MB dump with the calibration checksum region and the family
    /// string, nothing else.
    pub fn blank_dump() -> Vec<u8> {
        let mut d = vec![0xFFu8; layout::DUMP_SIZE];
        d[CAL_START..=CAL_END].fill(0);
        let desc = 0xE0280;
        d[desc - 8..desc - 4].copy_from_slice(&(layout::MEMORY_BASE + CAL_START as u32).to_be_bytes());
        d[desc - 4..desc].copy_from_slice(&(layout::MEMORY_BASE + CAL_END as u32).to_be_bytes());
        d[desc..desc + 8].copy_from_slice(&layout::CHECKSUM_MAGIC);
        let fam = b",99/1/EDC16C31/999/X000/X/";
        d[0xFE639..0xFE639 + fam.len()].copy_from_slice(fam);
        d
    }

    fn words(v: &[i16]) -> Vec<u8> {
        v.iter().flat_map(|w| w.to_be_bytes()).collect()
    }

    /// `Kf` block from rows (rows = first axis).
    pub fn kf(x: &[i16], y: &[i16], rows: &[Vec<i16>]) -> Vec<u8> {
        let mut w = vec![x.len() as i16, y.len() as i16];
        w.extend_from_slice(x);
        w.extend_from_slice(y);
        for r in rows {
            w.extend_from_slice(r);
        }
        words(&w)
    }

    pub fn kl(x: &[i16], z: &[i16]) -> Vec<u8> {
        let mut w = vec![x.len() as i16];
        w.extend_from_slice(x);
        w.extend_from_slice(z);
        words(&w)
    }

    pub fn write(d: &mut [u8], off: usize, rec: &[u8]) -> usize {
        d[off..off + rec.len()].copy_from_slice(rec);
        off + rec.len()
    }

    const RPM16: [i16; 16] = [0, 850, 1008, 1250, 1500, 1750, 2000, 2250, 2500, 2750, 3003, 3507, 4000, 4250, 4405, 4750];
    const IQ16: [i16; 16] = [0, 500, 1000, 1500, 2000, 2500, 3000, 3500, 4000, 4500, 5000, 5500, 6000, 6300, 6500, 6700];

    fn boost_rows(top: i16) -> Vec<Vec<i16>> {
        (0..16).map(|r| (0..16).map(|c| (990 + (c * 250) + r * 5).min(top) as i16).collect()).collect()
    }

    fn soi_rows() -> Vec<Vec<i16>> {
        (0..16).map(|r| (0..16).map(|c| (-100 + r * 70 + c * 5) as i16).collect()).collect()
    }

    fn limiter_curve(first: i16, top: i16) -> (Vec<i16>, Vec<i16>) {
        let x: Vec<i16> = (0..21).map(|i| i * 250).collect(); // 0..5000
        let z: Vec<i16> = (0..21).map(|i| if i < 17 { top - i * 20 } else { first }).collect();
        (x, z)
    }

    #[test]
    fn foreign_and_blank_files_report_nothing() {
        let d = EDC16C31Detector::new();
        assert!(d.detect(&vec![0xFFu8; layout::DUMP_SIZE]).is_empty());
        assert!(d.detect(&vec![0u8; 0x200000]).is_empty(), "wrong size");
        assert!(d.detect(&blank_dump()).is_empty(), "region without blocks");
    }

    #[test]
    fn three_boost_targets_are_named_in_file_order() {
        let mut d = blank_dump();
        let mut off = 0xF9350;
        for _ in 0..3 {
            off = write(&mut d, off, &kf(&RPM16, &IQ16, &boost_rows(2400)));
        }
        let maps = EDC16C31Detector::new().detect(&d);
        let names: Vec<_> = maps.iter().filter_map(|m| m.name.clone()).collect();
        assert_eq!(names, ["Boost target map 1", "Boost target map 2", "Boost target map 3"]);
        let m = &maps[0];
        assert!(matches!(m.dimensions, MapDimensions::TwoDimensional { rows: 16, cols: 16 }));
        assert_eq!(m.y_axis_address, Some(0xF9350 + 4));
        assert_eq!(m.x_axis_address, Some(0xF9350 + 4 + 32));
        assert_eq!(m.address, 0xF9350 + 4 + 64);
        assert_eq!(m.is_little_endian, Some(false));
        assert_eq!(m.as_stored, Some(true));
        assert_eq!(m.unit.as_deref(), Some("mbar"));
        assert_eq!(m.y_label.as_deref(), Some("Engine speed (rpm)"));
    }

    #[test]
    fn an_incomplete_set_is_dropped_whole() {
        let mut d = blank_dump();
        let mut off = 0xF9350;
        for _ in 0..2 {
            off = write(&mut d, off, &kf(&RPM16, &IQ16, &boost_rows(2400)));
        }
        assert!(EDC16C31Detector::new().detect(&d).is_empty(), "two boost targets are not the set");
    }

    #[test]
    fn soi_needs_five_adjacent_maps() {
        let mut d = blank_dump();
        let mut off = 0xEA7C8;
        for _ in 0..4 {
            off = write(&mut d, off, &kf(&RPM16, &IQ16, &soi_rows()));
        }
        assert!(EDC16C31Detector::new().detect(&d).is_empty());
        write(&mut d, off, &kf(&RPM16, &IQ16, &soi_rows()));
        let maps = EDC16C31Detector::new().detect(&d);
        assert_eq!(maps.len(), 5);
        assert_eq!(maps[4].name.as_deref(), Some("Main injection SOI 5"));
        assert_eq!(maps[0].correction_factor, Some(0.0234375));
        assert!(matches!(maps[0].data_type, DataType::Int16));
    }

    #[test]
    fn a_remapped_boost_map_needs_tuned_mode() {
        let mut d = blank_dump();
        let mut off = 0xF9350;
        for _ in 0..3 {
            off = write(&mut d, off, &kf(&RPM16, &IQ16, &boost_rows(4300)));
        }
        assert!(EDC16C31Detector::new().detect(&d).is_empty());
        assert_eq!(EDC16C31Detector::new_tuned().detect(&d).len(), 3);
    }

    #[test]
    fn torque_limiter_curves_the_set_of_five_and_the_main_one() {
        let mut d = blank_dump();
        let (x, z) = limiter_curve(0, 3600);
        write(&mut d, 0xE66AE, &kl(&x, &z));
        let mut off = 0xE6D96;
        for top in [4000, 3900, 3800, 3700, 3600] {
            let (x, z) = limiter_curve(800, top);
            off = write(&mut d, off, &kl(&x, &z));
        }
        let maps = EDC16C31Detector::new().detect(&d);
        let names: Vec<_> = maps.iter().filter_map(|m| m.name.clone()).collect();
        assert_eq!(
            names,
            [
                "Torque limiter",
                "Torque limiter 1",
                "Torque limiter 2",
                "Torque limiter 3",
                "Torque limiter 4",
                "Engine speed limiter (torque)"
            ]
        );
        let c = &maps[0];
        assert!(matches!(c.dimensions, MapDimensions::OneDimensional { length: 21 }));
        assert_eq!(c.x_axis_address, Some(0xE66AE + 2));
        assert_eq!(c.address, 0xE66AE + 2 + 42);
        assert!(c.y_axis_address.is_none());
    }

    #[test]
    fn blocks_outside_the_calibration_region_are_ignored() {
        let mut d = blank_dump();
        let mut off = 0x10000; // program area, outside the region
        for _ in 0..3 {
            off = write(&mut d, off, &kf(&RPM16, &IQ16, &boost_rows(2400)));
        }
        assert!(EDC16C31Detector::new().detect(&d).is_empty());
    }
}
