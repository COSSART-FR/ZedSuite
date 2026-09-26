//! Bench of the EDC16C31 (BMW DDE) detector against REAL dumps.
//!
//! Dumps are never committed. What is committed is the expected map list
//! of the reference software (tests/fixtures/edc16c31/*.json): the detector
//! output at calibration time, every data address cross-checked against a
//! public map list of the same software.
//!
//! The tests are `#[ignore]`d by default and read the files from
//! environment variables:
//!
//!   ZEDSUITE_C31_DUMP_320D=/path/origine \
//!   ZEDSUITE_C31_DUMP_320D_TUNED=/path/stage1-egroff \
//!     cargo test --test edc16c31_dump -- --ignored --nocapture
//!
//! A variable left unset skips that file. The stock read must come back
//! with exactly the expected list, and a tune of the same software with
//! the same list: a remap changes values, not where the maps are.

use std::collections::BTreeMap;

use serde::Deserialize;
use zedsuite_lib::detector::ecu::bosch::edc16c31::layout;
use zedsuite_lib::detector::ecu::bosch::EDC16C31Detector;
use zedsuite_lib::detector::{ECUIdentifier, ECUType};
use zedsuite_lib::models::{DetectedMap, MapDimensions};

#[derive(Deserialize)]
struct Fixture {
    #[allow(dead_code)]
    source: String,
    maps: Vec<ExpectedMap>,
}

#[derive(Deserialize)]
struct ExpectedMap {
    name: String,
    address: u32,
    rows: usize,
    cols: usize,
}

fn fixture(tag: &str) -> Fixture {
    let path = format!("{}/tests/fixtures/edc16c31/{}.json", env!("CARGO_MANIFEST_DIR"), tag);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path, e))).expect("fixture json")
}

fn dump(var: &str) -> Option<Vec<u8>> {
    let p = std::env::var(var).ok()?;
    Some(std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {}", p, e)))
}

fn grid(m: &DetectedMap) -> (usize, usize) {
    match m.dimensions {
        MapDimensions::TwoDimensional { rows, cols } => (rows, cols),
        MapDimensions::OneDimensional { length } => (length, 1),
        MapDimensions::ThreeDimensional { .. } => (0, 0),
    }
}

fn be16(d: &[u8], a: u32) -> f64 {
    i16::from_be_bytes([d[a as usize], d[a as usize + 1]]) as f64
}

/// Axis values as exported, strictly increasing and plausible for the label.
fn check_axis(data: &[u8], m: &DetectedMap, addr: Option<u32>, n: usize, corr: Option<f64>, label: &Option<String>, errors: &mut Vec<String>) {
    let (Some(addr), Some(label)) = (addr, label.as_deref()) else { return };
    let f = corr.unwrap_or(1.0);
    let v: Vec<f64> = (0..n).map(|i| be16(data, addr + 2 * i as u32) * f).collect();
    let name = m.name.clone().unwrap_or_default();
    if !v.windows(2).all(|w| w[0] < w[1]) {
        errors.push(format!("{}: axis {} not increasing {:?}", name, label, v));
    }
    let (lo, hi) = match label {
        l if l.starts_with("Engine speed") => (0.0, 8000.0),
        l if l.starts_with("IQ") => (0.0, 200.0),
        l if l.starts_with("Torque") => (0.0, 1000.0),
        l if l.starts_with("Pedal") => (0.0, 100.0),
        l if l.starts_with("Rail pressure") => (0.0, 2500.0),
        l if l.contains("pressure (mbar)") => (0.0, 4500.0),
        _ => (f64::MIN, f64::MAX),
    };
    if v.iter().any(|&x| x < lo || x > hi) {
        errors.push(format!("{}: axis {} out of {}..{}: {:?}", name, label, lo, hi, v));
    }
}

fn bench(tag: &str, var: &str, expect_sw: &str) {
    let Some(data) = dump(var) else {
        eprintln!("{} not set, {} skipped", var, tag);
        return;
    };
    assert_eq!(data.len(), layout::DUMP_SIZE, "{}: expected a 1 MB dump", tag);

    let id = ECUIdentifier::identify(&data);
    println!("{}: {:?} {:?} conf {:.2} sw {:?}", tag, id.manufacturer, id.ecu_type, id.confidence, id.software_version);
    assert_eq!(id.ecu_type, ECUType::EDC16C31, "{}: identification", tag);
    assert_eq!(id.software_version.as_deref(), Some(expect_sw), "{}: software number", tag);

    let maps = EDC16C31Detector::new().detect(&data);
    for m in &maps {
        let (r, c) = grid(m);
        println!("  0x{:06X} {:>2}x{:<2} {}", m.address, r, c, m.name.clone().unwrap_or_default());
    }

    let expected = fixture("320d-1037361858").maps;
    let got: BTreeMap<String, &DetectedMap> = maps.iter().map(|m| (m.name.clone().unwrap_or_default(), m)).collect();
    assert_eq!(got.len(), maps.len(), "{}: duplicate map names", tag);

    let mut errors = Vec::new();
    for e in &expected {
        match got.get(&e.name) {
            None => errors.push(format!("missing: {}", e.name)),
            Some(m) => {
                if m.address != e.address {
                    errors.push(format!("{}: address 0x{:06X}, expected 0x{:06X}", e.name, m.address, e.address));
                }
                if grid(m) != (e.rows, e.cols) {
                    errors.push(format!("{}: grid {:?}, expected {}x{}", e.name, grid(m), e.rows, e.cols));
                }
            }
        }
    }
    for name in got.keys() {
        if !expected.iter().any(|e| &e.name == name) {
            errors.push(format!("unexpected: {}", name));
        }
    }
    for m in &maps {
        let (r, c) = grid(m);
        if c == 1 {
            check_axis(&data, m, m.x_axis_address, r, m.x_axis_correction, &m.x_label, &mut errors);
        } else {
            check_axis(&data, m, m.y_axis_address, r, m.y_axis_correction, &m.y_label, &mut errors);
            check_axis(&data, m, m.x_axis_address, c, m.x_axis_correction, &m.x_label, &mut errors);
        }
    }
    assert!(errors.is_empty(), "{}: {} difference(s):\n  {}", tag, errors.len(), errors.join("\n  "));
    println!("{}: {} maps, all at the expected addresses, axes plausible", tag, maps.len());
}

#[test]
#[ignore = "needs ZEDSUITE_C31_DUMP_320D"]
fn e46_320d_stock_read_matches_the_calibration() {
    bench("320d stock", "ZEDSUITE_C31_DUMP_320D", "1037361858");
}

#[test]
#[ignore = "needs ZEDSUITE_C31_DUMP_320D_TUNED"]
fn e46_320d_tune_keeps_every_map() {
    bench("320d tuned", "ZEDSUITE_C31_DUMP_320D_TUNED", "1037361858");
}

/// The stock read carries four checksum regions; the three that were read
/// whole are valid, the program region that runs into the unread area
/// (0x30000..) is not, and must not be "corrected".
#[test]
#[ignore = "needs ZEDSUITE_C31_DUMP_320D"]
fn e46_320d_checksum_regions() {
    let Some(data) = dump("ZEDSUITE_C31_DUMP_320D") else { return };
    let regions = layout::checksum_regions(&data);
    for r in &regions {
        println!("  0x{:06X}..0x{:06X} desc 0x{:06X} sum 0x{:08X} valid {}", r.start, r.end, r.descriptor, r.sum(&data), r.is_valid(&data));
    }
    assert_eq!(regions.len(), 4);
    let cal = layout::calibration_region(&data).expect("calibration region");
    assert_eq!((cal.start, cal.end), (0xE0180, 0xFEFFB));
    assert!(cal.is_valid(&data));
    assert_eq!(regions.iter().filter(|r| r.is_valid(&data)).count(), 3);
}

/// Prints every block of the calibration region with the family it was
/// recognised as: the view used to grow the family table.
#[test]
#[ignore = "needs ZEDSUITE_C31_DUMP_320D"]
fn prints_the_block_inventory() {
    let Some(data) = dump("ZEDSUITE_C31_DUMP_320D") else { return };
    let inv = EDC16C31Detector::new().inventory(&data);
    let named = inv.iter().filter(|e| e.spec.is_some()).count();
    for e in &inv {
        let b = &e.block;
        let second = b.second.as_ref().map(|a| format!("[{:2}] {:>6}..{:<6}", a.len(), a.first(), a.last())).unwrap_or_else(|| "-".into());
        println!(
            "0x{:06X} {:<38} first [{:2}] {:>6}..{:<6} second {:<22} {}x{} min {} max {}",
            b.addr,
            e.spec.map(|s| s.name).unwrap_or("-"),
            b.first.len(),
            b.first.first(),
            b.first.last(),
            second,
            b.rows(),
            b.cols(),
            b.min(),
            b.max()
        );
    }
    println!("{} blocks, {} named", inv.len(), named);
}
