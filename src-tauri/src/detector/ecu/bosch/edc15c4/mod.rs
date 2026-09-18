// Bosch EDC15C4 detector (BMW DDE 4.0 - M57 / M47 common rail, 512 KB).
// Calibrated against the Bosch A2L of the reference project (P079.VB4,
// damos 4ZB1379 and 6ZC1179) - see docs/PORTING-EDC15C4.md.
//
// How it reads a file
// -------------------
// 1. layout.rs locates the signed calibration block (V2.0 signature) and
//    walks its self-describing records [id][n][axis][id][m][axis][data].
//    Nothing is guessed: a record is consumed whole, no address is fixed.
// 2. families.rs names the inline records from their grid, axis value
//    ranges, data ranges and file-order sequence. The names and factors
//    come from the A2L; the rules come from the three files of the corpus.
// 3. group.rs handles the injection block, whose maps are bare data blocks
//    sharing one cluster of axes: it finds the cluster and reads the maps
//    at their fixed offsets from it, each checked against a physical
//    window.
//
// What is NOT shared with the VAG modules: the axis id tables (an id is a
// RAM address here, it moves between builds), the VAG pattern list, and
// the v4.1 checksum - the block is signed "V2.0" and the frontend keeps
// EDC15C4 away from the VAG checksum corrector (src/lib/ecu-family.ts).
//
// Safety contract
// ---------------
// `detect()` refuses any size other than 512 KB and any file without a V2.0
// block. Families that come in a fixed sequence (six durations, three smoke
// limiters, three driver wish maps, three torque limiter curves) are only
// reported when the sequence is complete, and a rule that fires twice for a
// unique family reports nothing. Per CONTRIBUTING.md: "fewer maps that are
// right rather than more maps that are almost right".

pub mod families;
pub mod group;
pub mod layout;
pub mod spec;

pub use layout::{parse_records, Axis, Edc15c4Layout, Record};
pub use spec::MapSpec;

use crate::models::{DataType, DetectedMap, MapDimensions};

/// One record of the calibration block with the family it was recognised
/// as (None = unclassified). Used by the bench and the fixture tests.
#[derive(Debug, Clone)]
pub struct InventoryEntry {
    pub record: Record,
    pub spec: Option<MapSpec>,
}

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

    /// Tuned mode widens the data windows (a remapped map exceeds the stock
    /// ceilings) without touching the axis rules.
    pub fn new_tuned() -> Self {
        Self { tuned: true }
    }

    /// Every inline record of the calibration block, classified. Empty
    /// when the file carries no V2.0 block.
    pub fn inventory(&self, data: &[u8]) -> Vec<InventoryEntry> {
        let Some(layout) = Edc15c4Layout::detect(data) else {
            return Vec::new();
        };
        let records = parse_records(data, layout.block_start, layout.block_end);
        let classified = families::classify(&records, self.tuned);
        let mut specs: Vec<Option<MapSpec>> = vec![None; records.len()];
        for c in classified {
            specs[c.index] = Some(c.spec);
        }
        records
            .into_iter()
            .zip(specs)
            .map(|(record, spec)| InventoryEntry { record, spec })
            .collect()
    }

    /// The maps of the file. Never falls back to a VAG detector.
    pub fn detect(&self, data: &[u8]) -> Vec<DetectedMap> {
        if data.len() != layout::DUMP_SIZE {
            return Vec::new();
        }
        let Some(layout) = Edc15c4Layout::detect(data) else {
            return Vec::new();
        };
        let mut maps = Vec::new();

        for entry in self.inventory(data) {
            let Some(spec) = entry.spec else { continue };
            let r = &entry.record;
            maps.push(self.build(
                &spec,
                &layout,
                r.data_addr,
                r.rows(),
                r.cols(),
                r.first.values_addr,
                r.second.as_ref().map(|a| a.values_addr),
            ));
        }

        if let Some(cluster) = group::find_cluster(data, layout.block_start, layout.block_end) {
            for g in group::find_group_maps(data, &cluster, layout.block_end, self.tuned) {
                maps.push(self.build(
                    &g.def.spec,
                    &layout,
                    g.data_addr,
                    g.def.rows,
                    g.def.cols,
                    g.y_axis_addr,
                    Some(g.x_axis_addr),
                ));
            }
        } else {
            log::debug!("EDC15C4: shared-axis cluster not found, injection group maps skipped");
        }

        maps.sort_by_key(|m| m.address);
        maps
    }

    /// rows = first axis (Y), cols = second axis (X); little-endian u16.
    #[allow(clippy::too_many_arguments)]
    fn build(
        &self,
        spec: &MapSpec,
        layout: &Edc15c4Layout,
        data_addr: usize,
        rows: usize,
        cols: usize,
        y_axis_addr: usize,
        x_axis_addr: Option<usize>,
    ) -> DetectedMap {
        let dims = if cols == 1 {
            MapDimensions::OneDimensional { length: rows }
        } else {
            MapDimensions::TwoDimensional { rows, cols }
        };
        let data_type = if spec.signed { DataType::Int16 } else { DataType::UInt16 };
        let mut m = DetectedMap::new(data_addr as u32, rows * cols * 2, dims, data_type);
        m.id = format!("edc15c4_{:06X}", data_addr);
        m.name = Some(spec.name.to_string());
        m.category = Some(spec.category.display_name().to_string());
        m.subcategory = Some(spec.subcategory.to_string());
        m.description = Some(spec.description.to_string());
        m.unit = Some(spec.unit.to_string());
        m.correction_factor = Some(spec.z_factor);
        m.offset = Some(spec.z_offset);
        m.confidence = spec.confidence;
        m.is_little_endian = Some(true);
        m.codeblock_id = Some(1);
        m.codeblock_start_address = Some(layout.block_start as u32);
        m.codeblock_end_address = Some(layout.block_end as u32);
        // A curve has its single axis along the rows; the app reads a 1D
        // map's axis from x_axis_address, so the row axis goes there.
        match (cols, x_axis_addr, spec.x) {
            (1, _, _) => {
                m.x_axis_address = Some(y_axis_addr as u32);
                m.x_label = Some(spec.y.label.to_string());
                m.x_axis_correction = Some(spec.y.factor);
                m.x_axis_offset = Some(spec.y.offset);
            }
            (_, Some(xa), Some(x)) => {
                m.y_axis_address = Some(y_axis_addr as u32);
                m.y_label = Some(spec.y.label.to_string());
                m.y_axis_correction = Some(spec.y.factor);
                m.y_axis_offset = Some(spec.y.offset);
                m.x_axis_address = Some(xa as u32);
                m.x_label = Some(x.label.to_string());
                m.x_axis_correction = Some(x.factor);
                m.x_axis_offset = Some(x.offset);
            }
            _ => {
                m.y_axis_address = Some(y_axis_addr as u32);
                m.y_label = Some(spec.y.label.to_string());
                m.y_axis_correction = Some(spec.y.factor);
                m.y_axis_offset = Some(spec.y.offset);
            }
        }
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

    fn record_1d(id: u16, ax: &[u16], z: &[u16]) -> Vec<u8> {
        let mut b = Vec::new();
        put16(&mut b, id);
        put16(&mut b, ax.len() as u16);
        ax.iter().for_each(|&v| put16(&mut b, v));
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

    fn write(data: &mut [u8], off: usize, rec: &[u8]) -> usize {
        data[off..off + rec.len()].copy_from_slice(rec);
        off + rec.len()
    }

    const RAIL: [u16; 15] = [1190, 1200, 2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000, 10000, 11000, 12000, 13500, 14500];
    const IQ32: [u16; 32] = [
        0, 2, 50, 100, 150, 200, 270, 340, 400, 500, 600, 700, 800, 950, 1250, 1500, 1750, 2000, 2250,
        2500, 2750, 3000, 3250, 3500, 3750, 4000, 4250, 4500, 5000, 5500, 6000, 7000,
    ];
    const RPM16: [u16; 16] = [0, 850, 1008, 1250, 1500, 1750, 2000, 2250, 2500, 2750, 3003, 3507, 4000, 4200, 4408, 4600];
    const IQ10: [u16; 10] = [0, 1000, 1200, 1500, 2000, 2500, 3500, 4000, 4500, 5000];
    const RPM19: [u16; 19] = [500, 600, 700, 800, 1000, 1250, 1500, 1750, 2000, 2250, 2500, 2750, 3000, 3250, 3500, 3750, 4000, 4500, 5000];
    const AIR16: [u16; 16] = [2000, 2500, 3000, 3500, 3750, 4000, 4500, 5000, 5500, 6000, 6200, 6500, 7000, 7500, 8000, 8500];

    fn duration_record() -> Vec<u8> {
        let mut z = Vec::new();
        for r in 0..RAIL.len() {
            for c in 0..IQ32.len() {
                let v = if c == 0 { 0 } else { (c as u16 * 120).saturating_sub(r as u16 * 40).max(20) };
                z.push(v.min(5000));
            }
        }
        record_2d(0xC032, &RAIL, 0xD900, &IQ32, &z)
    }

    fn boost_record() -> Vec<u8> {
        let mut z = Vec::new();
        for r in 0..RPM16.len() {
            for c in 0..IQ10.len() {
                z.push(990 + (c as u16 * 120) + (r as u16 * 5));
            }
        }
        record_2d(0xC016, &RPM16, 0xC314, &IQ10, &z)
    }

    fn smoke_record(peak: u16) -> Vec<u8> {
        let mut z = Vec::new();
        for r in 0..16 {
            for c in 0..16 {
                z.push((1700 + c as u16 * 280 + r as u16 * 10).min(peak));
            }
        }
        record_2d(0xC016, &RPM16, 0xC20C, &AIR16, &z)
    }

    fn limiter_curve(top: u16) -> Vec<u8> {
        let z: Vec<u16> = (0..19).map(|i| (3400 + i * 100).min(top)).collect();
        record_1d(0xC016, &RPM19, &z)
    }

    #[test]
    fn empty_and_wrong_size_files_report_nothing() {
        let d = EDC15C4Detector::new();
        assert!(d.detect(&vec![0xC3u8; layout::DUMP_SIZE]).is_empty(), "no block, no maps");
        assert!(d.detect(&vec![0xC3u8; 0x100000]).is_empty(), "wrong size");
        assert!(d.detect(&blank_dump()).is_empty(), "block without records");
    }

    #[test]
    fn six_durations_and_one_boost_target_on_a_synthetic_block() {
        let mut data = blank_dump();
        let mut off = 0x71800;
        for _ in 0..6 {
            off = write(&mut data, off, &duration_record());
        }
        write(&mut data, off, &boost_record());

        let maps = EDC15C4Detector::new().detect(&data);
        let names: Vec<String> = maps.iter().filter_map(|m| m.name.clone()).collect();
        assert_eq!(
            names,
            vec![
                "Injector duration 10 (no pilot)",
                "Injector duration 11 (no pilot)",
                "Injector duration 12 (no pilot)",
                "Injector duration 20 (with pilot)",
                "Injector duration 21 (with pilot)",
                "Injector duration 22 (with pilot)",
                "Boost target map (eco)",
            ]
        );
        let d0 = &maps[0];
        assert!(matches!(d0.dimensions, MapDimensions::TwoDimensional { rows: 15, cols: 32 }));
        assert_eq!(d0.y_axis_address, Some(0x71800 + 4));
        assert_eq!(d0.x_axis_address, Some(0x71800 + 4 + 30 + 4));
        assert_eq!(d0.address, 0x71800 + 4 + 30 + 4 + 64);
        assert_eq!(d0.is_little_endian, Some(true));
        assert_eq!(d0.y_axis_correction, Some(0.1));
        assert_eq!(d0.x_axis_correction, Some(0.01));
        assert_eq!(d0.x_label.as_deref(), Some("IQ (mm³/st)"));
    }

    #[test]
    fn a_partial_duration_set_is_dropped_whole() {
        let mut data = blank_dump();
        write(&mut data, 0x71800, &duration_record());
        assert!(EDC15C4Detector::new().detect(&data).is_empty());
    }

    #[test]
    fn smoke_limiters_are_named_by_file_order_and_only_as_a_set_of_three() {
        let mut data = blank_dump();
        let mut off = 0x71800;
        off = write(&mut data, off, &smoke_record(6289));
        off = write(&mut data, off, &smoke_record(6289));
        let two = EDC15C4Detector::new().detect(&data);
        assert!(two.is_empty(), "two smoke limiters are not a set");
        write(&mut data, off, &smoke_record(7300));
        let three = EDC15C4Detector::new().detect(&data);
        let names: Vec<String> = three.iter().filter_map(|m| m.name.clone()).collect();
        assert_eq!(names, vec!["Smoke limiter (dynamic)", "Smoke limiter", "Smoke limiter (low range)"]);
        assert_eq!(three[0].x_label.as_deref(), Some("Airflow (mg/st)"));
        assert_eq!(three[0].x_axis_correction, Some(0.1));
    }

    #[test]
    fn torque_limiter_curves_need_a_run_of_three() {
        let mut data = blank_dump();
        let mut off = 0x71800;
        off = write(&mut data, off, &limiter_curve(5000));
        // something else in between breaks the run
        off = write(&mut data, off, &boost_record());
        off = write(&mut data, off, &limiter_curve(5250));
        off = write(&mut data, off, &limiter_curve(5700));
        off = write(&mut data, off, &limiter_curve(5200));
        let z16: Vec<u16> = (0..16).map(|i| 4000 + i * 100).collect();
        write(&mut data, off, &record_1d(0xC016, &RPM16, &z16));
        let maps = EDC15C4Detector::new().detect(&data);
        let names: Vec<String> = maps.iter().filter_map(|m| m.name.clone()).collect();
        assert_eq!(
            names,
            vec![
                "Boost target map (eco)",
                "Torque limiter (pull-away)",
                "Torque limiter (raised)",
                "Torque limiter (normal)",
                "Torque limiter (low range)",
            ]
        );
        let curve = &maps[1];
        assert!(matches!(curve.dimensions, MapDimensions::OneDimensional { length: 19 }));
        assert_eq!(curve.x_label.as_deref(), Some("Engine speed (rpm)"));
        assert!(curve.y_axis_address.is_none());
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
        write(&mut data, 0x71800, &record_2d(0xC016, &RPM16, 0xC314, &IQ10, &z));
        assert!(EDC15C4Detector::new().detect(&data).is_empty());
        assert_eq!(EDC15C4Detector::new_tuned().detect(&data).len(), 1);
    }

    #[test]
    fn group_maps_come_from_the_shared_axis_cluster() {
        let mut data = blank_dump();
        group::tests::write_cluster(&mut data, 0x74F38);
        // rail target 300..1350 bar (16 x 16)
        for i in 0..256 {
            let v: u16 = 3000 + (i as u16) * 40;
            data[0x74F38 + 0xF0 + 2 * i..0x74F38 + 0xF0 + 2 * i + 2].copy_from_slice(&v.to_le_bytes());
        }
        let maps = EDC15C4Detector::new().detect(&data);
        let rail = maps.iter().find(|m| m.name.as_deref() == Some("Rail pressure target map")).expect("rail");
        assert_eq!(rail.address, 0x74F38 + 0xF0);
        assert_eq!(rail.y_axis_address, Some(0x74F38 + 4));
        assert_eq!(rail.x_axis_address, Some(0x74F38 + 0xA0));
        assert_eq!(rail.correction_factor, Some(0.1));
        assert_eq!(rail.unit.as_deref(), Some("bar"));
        // the untouched C3 blocks around it are not reported
        assert_eq!(maps.len(), 1);
    }
}
