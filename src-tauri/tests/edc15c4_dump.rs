//! Bench of the EDC15C4 (BMW DDE 4.0) detector against REAL dumps.
//!
//! Dumps are never committed. What is committed is the expected map list
//! of each file of the corpus (tests/fixtures/edc15c4/*.json): for the two
//! damos builds the addresses come straight from the Bosch A2L, for the
//! 525d read they are the detector output at calibration time, cross
//! checked record by record against the 4ZB1379 A2L.
//!
//! The tests are `#[ignore]`d by default and read the files from
//! environment variables, one per build:
//!
//!   ZEDSUITE_C4_DUMP_4ZB1379=/path/4ZB1379_TSW-V2.40_0090799.ori \
//!   ZEDSUITE_C4_DUMP_6ZC1179=/path/6ZC1179_530D.org \
//!   ZEDSUITE_C4_DUMP_525D=/path/origine525d.bin \
//!     cargo test --test edc15c4_dump -- --ignored --nocapture
//!
//! A variable left unset skips that file. Each file must come back with
//! exactly its expected list: same names, same data addresses, same grids,
//! nothing more, nothing less.

use std::collections::BTreeMap;

use serde::Deserialize;
use zedsuite_lib::detector::ecu::bosch::EDC15C4Detector;
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
    let path = format!("{}/tests/fixtures/edc15c4/{}.json", env!("CARGO_MANIFEST_DIR"), tag);
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

fn bench(tag: &str, var: &str, expect_sw: Option<&str>) {
    let Some(data) = dump(var) else {
        eprintln!("{} not set, {} skipped", var, tag);
        return;
    };
    assert_eq!(data.len(), 0x80000, "{}: expected a 512 KB dump", tag);

    let id = ECUIdentifier::identify(&data);
    println!("{}: {:?} {:?} conf {:.2} sw {:?} variant {:?}", tag, id.manufacturer, id.ecu_type, id.confidence, id.software_version, id.variant);
    assert_eq!(id.ecu_type, ECUType::EDC15C4, "{}: identification", tag);
    if let Some(sw) = expect_sw {
        assert_eq!(id.software_version.as_deref(), Some(sw), "{}: software number", tag);
    }

    let maps = EDC15C4Detector::new().detect(&data);
    for m in &maps {
        let (r, c) = grid(m);
        println!("  0x{:06X} {:>2}x{:<2} {}", m.address, r, c, m.name.clone().unwrap_or_default());
    }

    let expected = fixture(tag).maps;
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
    assert!(errors.is_empty(), "{}: {} difference(s):\n  {}", tag, errors.len(), errors.join("\n  "));
    println!("{}: {} maps, all at the expected addresses", tag, maps.len());
}

#[test]
#[ignore = "needs ZEDSUITE_C4_DUMP_4ZB1379"]
fn damos_4zb1379_matches_its_a2l() {
    bench("4ZB1379", "ZEDSUITE_C4_DUMP_4ZB1379", Some("1037351513"));
}

#[test]
#[ignore = "needs ZEDSUITE_C4_DUMP_6ZC1179"]
fn damos_6zc1179_matches_its_a2l() {
    // Wiped ASCII header on that file: identified by structure, no SW number.
    bench("6ZC1179", "ZEDSUITE_C4_DUMP_6ZC1179", None);
}

#[test]
#[ignore = "needs ZEDSUITE_C4_DUMP_525D"]
fn e39_525d_read_matches_the_calibration() {
    bench("525d", "ZEDSUITE_C4_DUMP_525D", Some("1037351632"));
}

/// Prints every inline record of a file with the family it was recognised
/// as: the view used to grow the family table against a reference.
#[test]
#[ignore = "needs ZEDSUITE_C4_DUMP_525D"]
fn prints_the_record_inventory() {
    let Some(data) = dump("ZEDSUITE_C4_DUMP_525D") else {
        panic!("set ZEDSUITE_C4_DUMP_525D to a 512 KB DDE 4.0 read");
    };
    let inv = EDC15C4Detector::new().inventory(&data);
    let mut named = 0;
    for e in &inv {
        let r = &e.record;
        let second = r
            .second
            .as_ref()
            .map(|a| format!("{:04X}[{}] {}..{}", a.id, a.len(), a.first(), a.last()))
            .unwrap_or_else(|| "-".to_string());
        if e.spec.is_some() {
            named += 1;
        }
        println!(
            "0x{:06X} {:<48} first {:04X}[{:2}] {:>5}..{:<5} second {:<26} data@0x{:06X} {}x{} min {} max {}",
            r.addr,
            e.spec.map(|s| s.name).unwrap_or("-"),
            r.first.id,
            r.first.len(),
            r.first.first(),
            r.first.last(),
            second,
            r.data_addr,
            r.rows(),
            r.cols(),
            r.min(),
            r.max()
        );
    }
    println!("{} records, {} named", inv.len(), named);
    assert!(inv.len() > 100, "the reference block carries ~210 records");
}
