//! Group maps of the injection block ("GKF" in the Bosch A2L).
//!
//! Most EDC15C4 maps carry their own axes (see layout.rs). The injection
//! system (rail pressure set point, pilot quantity, start of injection) is
//! different: its maps are bare data blocks (record layout `MSA15_GKF4`,
//! `FNC_VALUES` only) that share a set of common axes (`AXIS_PTS`
//! zuwXstzv / zuwYstzv* / zuwYEakt*). The shared axes form a **cluster**
//! of eight consecutive axis records with a fixed shape, and the data
//! blocks sit at **fixed offsets from that cluster**.
//!
//! Confirmed identical on the three builds of the corpus (damos 4ZB1379,
//! damos 6ZC1179, the 525d read), A2L addresses in hand:
//!
//! ```text
//! +0x000  zuwXstzv    16 × rpm      (dzmNmit)     values at +0x004
//! +0x024  zuwXstzv6    6 × rpm      start case
//! +0x034  zuwXstzv4    4 × rpm      start case
//! +0x040  zuwYstzv6    6 × mm³      (mrmM_EMTS)
//! +0x050  zuwYstzv8    8 × mm³      (mrmM_EMTS)   values at +0x054
//! +0x064  zuwYstzv16  16 × mm³      (mrmM_EMTS)   values at +0x068
//! +0x088  zuwYEakt8    8 × mm³      (mrmM_EAKT)   values at +0x08C
//! +0x09C  zuwYEakt16  16 × mm³      (mrmM_EAKT)   values at +0x0A0
//! +0x0C0  end of the cluster
//! +0x0F0  zuwPQGWKF   16 × 16  rail pressure set point  (Xstzv, YEakt16)
//! +0x6F0  zuwPQmaxKF  16 × 8   rail pressure maximum    (Xstzv, YEakt8)
//! +0x2542 zuwABVGWKF  16 × 16  pilot SOI, relative      (Xstzv, Ystzv16)
//! +0x2A3E zuwMEVGWKF  16 × 16  pilot quantity           (Xstzv, Ystzv16)
//! +0x2D22 zuwMVEmxKF  16 × 8   pilot quantity maximum   (Xstzv, Ystzv8)
//! +0x2E66 zuwABHmxKF  16 × 8   earliest main SOI        (Xstzv, Ystzv8)
//! +0x2F66 zuwABHG1KF  16 × 16  main SOI with pilot      (Xstzv, Ystzv16)
//! +0x3166 zuwABHG2KF  16 × 16  main SOI without pilot   (Xstzv, Ystzv16)
//! ```
//!
//! The layout is rigid up to +0x39C2 on both A2Ls (the two builds diverge
//! by 0x44 bytes only after that point). Every block is still checked
//! against a physical range before it is reported, so a build that moved
//! the cluster loses the whole group rather than reporting garbage.

use super::layout::{axis_at, Axis};
use super::spec::{self, MapSpec};

/// Expected lengths of the eight axes of the cluster, in file order.
const CLUSTER_SHAPE: [usize; 8] = [16, 6, 4, 6, 8, 16, 8, 16];
/// Size of the cluster (last axis end).
const CLUSTER_SIZE: usize = 0xC0;

/// Offsets of the shared axes' VALUES inside the cluster.
const XSTZV: usize = 0x004;
const YSTZV8: usize = 0x054;
const YSTZV16: usize = 0x068;
const YEAKT8: usize = 0x08C;
const YEAKT16: usize = 0x0A0;

#[derive(Debug, Clone, Copy)]
pub struct GroupMapDef {
    pub rel: usize,
    pub rows: usize,
    pub cols: usize,
    /// Offset of the column axis values inside the cluster.
    pub x_axis_rel: usize,
    pub spec: MapSpec,
    /// Accepted raw range on a stock file (signed values compared as i16).
    pub stock: (i32, i32),
    pub tuned: (i32, i32),
}

pub const GROUP_MAPS: [GroupMapDef; 8] = [
    GroupMapDef { rel: 0x0F0, rows: 16, cols: 16, x_axis_rel: YEAKT16, spec: spec::RAIL_TARGET, stock: (2000, 14500), tuned: (2000, 16000) },
    GroupMapDef { rel: 0x6F0, rows: 16, cols: 8, x_axis_rel: YEAKT8, spec: spec::RAIL_MAX, stock: (2000, 14500), tuned: (2000, 16000) },
    GroupMapDef { rel: 0x2542, rows: 16, cols: 16, x_axis_rel: YSTZV16, spec: spec::SOI_PILOT, stock: (0, 3900), tuned: (0, 3900) },
    GroupMapDef { rel: 0x2A3E, rows: 16, cols: 16, x_axis_rel: YSTZV16, spec: spec::PILOT_QTY, stock: (0, 2000), tuned: (0, 3000) },
    GroupMapDef { rel: 0x2D22, rows: 16, cols: 8, x_axis_rel: YSTZV8, spec: spec::PILOT_QTY_MAX, stock: (0, 3000), tuned: (0, 4000) },
    GroupMapDef { rel: 0x2E66, rows: 16, cols: 8, x_axis_rel: YSTZV8, spec: spec::SOI_MAIN_EARLIEST, stock: (-1000, 3000), tuned: (-1000, 3000) },
    GroupMapDef { rel: 0x2F66, rows: 16, cols: 16, x_axis_rel: YSTZV16, spec: spec::SOI_MAIN_WITH_PILOT, stock: (-1000, 3000), tuned: (-1000, 3000) },
    GroupMapDef { rel: 0x3166, rows: 16, cols: 16, x_axis_rel: YSTZV16, spec: spec::SOI_MAIN_NO_PILOT, stock: (-1000, 3000), tuned: (-1000, 3000) },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cluster {
    pub base: usize,
    pub axes: Vec<Axis>,
}

impl Cluster {
    pub fn rpm_axis_addr(&self) -> usize {
        self.base + XSTZV
    }
}

/// Find the shared-axis cluster inside `[start, end)`: eight consecutive
/// axis records of shape [16, 6, 4, 6, 8, 16, 8, 16], the first three
/// rpm-shaped (500..1000 first, 4000..6000 / ~1000 last), the last five
/// quantity-shaped (0 first). Exactly one is expected per file.
pub fn find_cluster(data: &[u8], start: usize, end: usize) -> Option<Cluster> {
    let end = end.min(data.len());
    let mut found: Option<Cluster> = None;
    let mut off = start;
    while off + 4 <= end {
        if let Some(c) = cluster_at(data, off, end) {
            if found.is_some() {
                // Two clusters: layout not understood, refuse the group.
                return None;
            }
            off = c.base + CLUSTER_SIZE;
            found = Some(c);
            continue;
        }
        off += 2;
    }
    found
}

fn cluster_at(data: &[u8], off: usize, end: usize) -> Option<Cluster> {
    let mut axes = Vec::with_capacity(8);
    let mut o = off;
    for (i, &len) in CLUSTER_SHAPE.iter().enumerate() {
        let a = axis_at(data, o, end)?;
        if a.len() != len {
            return None;
        }
        let ok = if i < 3 {
            (100..=1100).contains(&a.first()) && (800..=6000).contains(&a.last())
        } else {
            a.first() == 0 && (3000..=10_000).contains(&a.last())
        };
        if !ok {
            return None;
        }
        o = a.values_addr + 2 * a.len();
        axes.push(a);
    }
    if o - off != CLUSTER_SIZE {
        return None;
    }
    if !(4000..=6000).contains(&axes[0].last()) {
        return None;
    }
    Some(Cluster { base: off, axes })
}

/// A group map located and validated on the file.
#[derive(Debug, Clone)]
pub struct GroupMap {
    pub def: GroupMapDef,
    pub data_addr: usize,
    pub y_axis_addr: usize,
    pub x_axis_addr: usize,
}

/// Locate every group map of `GROUP_MAPS` relative to the cluster and keep
/// those whose values fit the physical window.
pub fn find_group_maps(data: &[u8], cluster: &Cluster, block_end: usize, tuned: bool) -> Vec<GroupMap> {
    let mut out = Vec::new();
    for def in GROUP_MAPS.iter() {
        let data_addr = cluster.base + def.rel;
        let count = def.rows * def.cols;
        if data_addr + 2 * count > block_end.min(data.len()) {
            continue;
        }
        let (lo, hi) = if tuned { def.tuned } else { def.stock };
        let fits = (0..count).all(|i| {
            let raw = u16::from_le_bytes([data[data_addr + 2 * i], data[data_addr + 2 * i + 1]]);
            let v = if def.spec.signed { raw as i16 as i32 } else { raw as i32 };
            (lo..=hi).contains(&v)
        });
        if !fits {
            log::debug!("EDC15C4 group map {} at 0x{:X} out of range, skipped", def.spec.name, data_addr);
            continue;
        }
        out.push(GroupMap {
            def: *def,
            data_addr,
            y_axis_addr: cluster.rpm_axis_addr(),
            x_axis_addr: cluster.base + def.x_axis_rel,
        });
    }
    out
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn put16(buf: &mut [u8], off: usize, v: u16) {
        buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }

    fn axis(buf: &mut [u8], off: usize, id: u16, vals: &[u16]) -> usize {
        put16(buf, off, id);
        put16(buf, off + 2, vals.len() as u16);
        for (i, &v) in vals.iter().enumerate() {
            put16(buf, off + 4 + 2 * i, v);
        }
        off + 4 + 2 * vals.len()
    }

    /// Writes a cluster shaped like the real one at `base`.
    pub fn write_cluster(buf: &mut [u8], base: usize) {
        let rpm16: Vec<u16> = (0..16).map(|i| 500 + i * 290).collect(); // 500..4850
        let rpm6: Vec<u16> = (0..6).map(|i| 150 + i * 170).collect();
        let rpm4: Vec<u16> = (0..4).map(|i| 250 + i * 250).collect();
        let q6: Vec<u16> = (0..6).map(|i| i * 1000).collect();
        let q8: Vec<u16> = (0..8).map(|i| i * 800).collect();
        let q16: Vec<u16> = (0..16).map(|i| i * 400).collect();
        let mut o = base;
        o = axis(buf, o, 0xC016, &rpm16);
        o = axis(buf, o, 0xC016, &rpm6);
        o = axis(buf, o, 0xC016, &rpm4);
        o = axis(buf, o, 0xC062, &q6);
        o = axis(buf, o, 0xC062, &q8);
        o = axis(buf, o, 0xC062, &q16);
        o = axis(buf, o, 0xC1CE, &q8);
        o = axis(buf, o, 0xC1CE, &q16);
        assert_eq!(o - base, CLUSTER_SIZE);
    }

    #[test]
    fn cluster_is_found_and_group_maps_validated() {
        let mut buf = vec![0xC3u8; 0x8000];
        write_cluster(&mut buf, 0x100);
        // rail target: 300..1350 bar
        for i in 0..256 {
            put16(&mut buf, 0x100 + 0x0F0 + 2 * i, 3000 + (i as u16) * 40);
        }
        // main SOI with pilot: signed, -5..+20 degCA
        for i in 0..256 {
            put16(&mut buf, 0x100 + 0x2F66 + 2 * i, ((i as i32 * 4) - 200) as i16 as u16);
        }
        let c = find_cluster(&buf, 0, buf.len()).expect("cluster");
        assert_eq!(c.base, 0x100);
        let maps = find_group_maps(&buf, &c, buf.len(), false);
        let names: Vec<&str> = maps.iter().map(|m| m.def.spec.name).collect();
        assert!(names.contains(&"Rail pressure target map"));
        assert!(names.contains(&"Main injection SOI (with pilot)"));
        // the C3-filled blocks (0xC3C3 = 50115, or -15421 signed) are refused
        assert!(!names.contains(&"Rail pressure maximum map"));
        assert!(!names.contains(&"Pilot injection quantity"));
        let rail = maps.iter().find(|m| m.def.spec.name == "Rail pressure target map").unwrap();
        assert_eq!(rail.y_axis_addr, 0x104);
        assert_eq!(rail.x_axis_addr, 0x100 + 0xA0);
    }

    #[test]
    fn two_clusters_refuse_the_group() {
        let mut buf = vec![0xC3u8; 0x8000];
        write_cluster(&mut buf, 0x100);
        write_cluster(&mut buf, 0x4000);
        assert!(find_cluster(&buf, 0, buf.len()).is_none());
    }
}
