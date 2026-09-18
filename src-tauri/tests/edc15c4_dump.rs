//! Fixture test for the EDC15C4 (BMW DDE 4.0) detector, run against a REAL
//! dump.
//!
//! Dumps are never committed, so this test is `#[ignore]`d by default and
//! reads the file from an environment variable:
//!
//!   ZEDSUITE_C4_DUMP=/path/to/original.bin \
//!     cargo test --test edc15c4_dump -- --ignored --nocapture
//!
//! The expected addresses are the ones confirmed on the reference file
//! (BMW E39 525d M57D25, TSW V2.40 090799 1418 C4B/ESB/43, Bosch SW
//! 1037351632). Another software build will move them: the test is then
//! expected to fail on the addresses while still finding the same families,
//! which is the signal wanted when a second file joins the corpus.

use zedsuite_lib::detector::ecu::bosch::edc15c4::Family;
use zedsuite_lib::detector::ecu::bosch::EDC15C4Detector;
use zedsuite_lib::detector::{ECUIdentifier, ECUType};

fn dump() -> Option<Vec<u8>> {
    let p = std::env::var("ZEDSUITE_C4_DUMP").ok()?;
    std::fs::read(p).ok()
}

#[test]
#[ignore = "needs ZEDSUITE_C4_DUMP"]
fn identifies_the_reference_dump_as_edc15c4() {
    let Some(data) = dump() else {
        panic!("set ZEDSUITE_C4_DUMP to a 512 KB DDE 4.0 read");
    };
    let id = ECUIdentifier::identify(&data);
    println!("{:?} {:?} conf {:.2} sw {:?} variant {:?}", id.manufacturer, id.ecu_type, id.confidence, id.software_version, id.variant);
    assert_eq!(id.ecu_type, ECUType::EDC15C4);
    assert_eq!(id.software_version.as_deref(), Some("1037351632"));
}

#[test]
#[ignore = "needs ZEDSUITE_C4_DUMP"]
fn finds_every_calibrated_family_on_the_reference_dump() {
    let Some(data) = dump() else {
        panic!("set ZEDSUITE_C4_DUMP to a 512 KB DDE 4.0 read");
    };
    assert_eq!(data.len(), 0x80000, "expected a 512 KB dump");

    let maps = EDC15C4Detector::new().detect(&data);
    for m in &maps {
        println!(
            "0x{:06X} {:<24} {:?} y@{:06X} x@{:06X} conf {:.2}",
            m.address,
            m.name.clone().unwrap_or_default(),
            m.dimensions,
            m.y_axis_address.unwrap_or(0),
            m.x_axis_address.unwrap_or(0),
            m.confidence
        );
    }

    // (name, record address, data address, rows, cols)
    let expected: &[(&str, u32, usize, usize)] = &[
        ("Injector duration 00", 0x075BEA, 15, 32),
        ("Injector duration 01", 0x076010, 15, 32),
        ("Injector duration 02", 0x076436, 15, 32),
        ("Injector duration 03", 0x07685C, 15, 32),
        ("Injector duration 04", 0x076C82, 15, 32),
        ("Injector duration 05", 0x0770A8, 15, 32),
        ("Boost target map", 0x0742CE, 16, 10),
    ];
    assert_eq!(maps.len(), expected.len(), "unexpected map count");
    for (name, data_addr, rows, cols) in expected {
        let m = maps
            .iter()
            .find(|m| m.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("{} not found", name));
        assert_eq!(m.address, *data_addr, "{} data address", name);
        match m.dimensions {
            zedsuite_lib::models::MapDimensions::TwoDimensional { rows: r, cols: c } => {
                assert_eq!((r, c), (*rows, *cols), "{} grid", name);
            }
            _ => panic!("{} is not 2D", name),
        }
    }
}

/// Prints the whole record inventory of the block with the family each
/// record was recognised as. This is the bench view used to grow the
/// family table: run it, compare with a reference, promote a hypothesis.
#[test]
#[ignore = "needs ZEDSUITE_C4_DUMP"]
fn prints_the_record_inventory() {
    let Some(data) = dump() else {
        panic!("set ZEDSUITE_C4_DUMP to a 512 KB DDE 4.0 read");
    };
    let inv = EDC15C4Detector::new().inventory(&data);
    let mut counts = std::collections::BTreeMap::new();
    for (r, f) in &inv {
        *counts.entry(format!("{:?}", f)).or_insert(0usize) += 1;
        let second = r
            .second
            .as_ref()
            .map(|a| format!("{:04X}[{}] {}..{}", a.id, a.len(), a.first(), a.last()))
            .unwrap_or_else(|| "-".to_string());
        println!(
            "0x{:06X} {:<38} first {:04X}[{}] {}..{}  second {:<26} data@0x{:06X} {}x{} min {} max {}",
            r.addr,
            f.label(),
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
    println!("{:?}", counts);
    assert!(inv.len() > 100, "the reference block carries ~200 records");
    assert_eq!(counts.get(&format!("{:?}", Family::InjectorDuration)), Some(&6));
    assert_eq!(counts.get(&format!("{:?}", Family::BoostTarget)), Some(&1));
}
