//! Memory layout of a Bosch EDC16C31 (BMW DDE) 1 MB dump and the
//! self-describing map blocks it carries.
//!
//! Confirmed on ONE software (BMW E46 320d 150 hp, M47TU, Bosch SW
//! 1037361858), three files: the stock read and two tunes of it.
//!
//! ```text
//! 0x000000..0x030000   boot + program (partial: the read stops at 0x30000)
//! 0x030000..0x0E0000   FF (not read)
//! 0x0E0000..0x100000   calibration
//! 0x0E0254             "10373618581974842A" (SW + BMW reference)
//! 0x0E6358             "01EDC16  System EDC16XY"
//! 0x0FE639             ",99/1/EDC16C31/999/X000/X/0X0000_000/..."
//! ```
//!
//! The dump is the image of the flash mapped at 0x800000 in the ECU. The
//! Bosch EDC16 checksum descriptors (`FA DE CA FE CA FE AF FE`, preceded by
//! the start and INCLUSIVE end addresses, big-endian) give absolute ECU
//! addresses; file offset = address - 0x800000. Unlike the VAG EDC16, the
//! descriptor is NOT at `start + 0x3C` (the calibration region starts at
//! 0x8E0180 and its descriptor sits at 0x8E0280), so the VAG rule that
//! derives the base from the descriptor position does not apply here.
//!
//! | Region (file)       | Descriptor | Sum on the stock read |
//! |---------------------|------------|-----------------------|
//! | 0x08000..0x0803F    | 0x082C4    | 0xD01FE500 (valid)    |
//! | 0x08180..0x1FFF7    | 0x0829C    | 0xD01FE500 (valid)    |
//! | 0x20000..0x5FFFB    | 0x2003C    | invalid: 0x30000.. not read |
//! | 0xE0180..0xFEFFB    | 0xE0280    | 0xD01FE500 (valid)    |
//!
//! The calibration region is the checksum region that carries the family
//! string `/EDC16C31/`. The maps are scanned inside it only.
//!
//! Every 2D map uses the Bosch record layout `Kf_Xs16_Ys16_Ws16`, the same
//! as the Mercedes EDC16CP31:
//!
//! ```text
//! +0x00  u16        nx          points of the first axis
//! +0x02  u16        ny          points of the second axis
//! +0x04  i16[nx]    first axis  strictly increasing
//! +....  i16[ny]    second axis strictly increasing
//! +....  i16[nx*ny] data        second axis is the fast index: z[x*ny + y]
//! ```
//!
//! and every curve `Kl_Xs16_Ws16`: `[n][axis n][data n]`. All big-endian
//! (MPC5xx). A block is allocated at its maximum size: a 19-point curve in
//! a 21-point slot is followed by 8 bytes of padding, and unused axis
//! points are sometimes filled with dummy increasing values (4510, 4520…).
//!
//! Since the second axis is the fast index, the app displays
//! `rows = first axis` (y_axis_address) and `cols = second axis`
//! (x_axis_address): file order = display order, no transposition.

/// A 1 MB read is the only supported size.
pub const DUMP_SIZE: usize = 0x100000;

/// ECU address of the first byte of the dump.
pub const MEMORY_BASE: u32 = 0x800000;

/// Bosch EDC16 checksum region descriptor signature.
pub const CHECKSUM_MAGIC: [u8; 8] = [0xFA, 0xDE, 0xCA, 0xFE, 0xCA, 0xFE, 0xAF, 0xFE];

/// Family token of the Bosch identification string.
pub const FAMILY_TOKEN: &[u8] = b"/EDC16C31/";

/// Sum every checksum region must reach (same invariant as the VAG EDC16).
pub const TARGET_SUM: u32 = 0xD01F_E500;

/// One checksum region, in file offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChecksumRegion {
    /// Offset of the descriptor signature.
    pub descriptor: usize,
    pub start: usize,
    /// Last byte covered (inclusive).
    pub end: usize,
}

impl ChecksumRegion {
    pub fn contains(&self, offset: usize) -> bool {
        (self.start..=self.end).contains(&offset)
    }

    /// Sum (mod 2^32) of the big-endian dwords of the region.
    pub fn sum(&self, data: &[u8]) -> u32 {
        data[self.start..=self.end]
            .chunks_exact(4)
            .fold(0u32, |acc, c| acc.wrapping_add(u32::from_be_bytes([c[0], c[1], c[2], c[3]])))
    }

    pub fn is_valid(&self, data: &[u8]) -> bool {
        self.sum(data) == TARGET_SUM
    }
}

/// Every checksum region of a 1 MB dump mapped at 0x800000. Regions whose
/// bounds fall outside the file, are not dword-sized, or are smaller than
/// 64 bytes are ignored.
pub fn checksum_regions(data: &[u8]) -> Vec<ChecksumRegion> {
    let mut out = Vec::new();
    if data.len() != DUMP_SIZE {
        return out;
    }
    let mut i = 8;
    while i + CHECKSUM_MAGIC.len() <= data.len() {
        if data[i..i + CHECKSUM_MAGIC.len()] == CHECKSUM_MAGIC {
            let start = u32::from_be_bytes([data[i - 8], data[i - 7], data[i - 6], data[i - 5]]);
            let end = u32::from_be_bytes([data[i - 4], data[i - 3], data[i - 2], data[i - 1]]);
            if start >= MEMORY_BASE && end > start {
                let s = (start - MEMORY_BASE) as usize;
                let e = (end - MEMORY_BASE) as usize;
                if e < data.len() && (e + 1 - s) % 4 == 0 && e + 1 - s >= 64 {
                    out.push(ChecksumRegion { descriptor: i, start: s, end: e });
                }
            }
        }
        i += 1;
    }
    out
}

/// The calibration region: the checksum region carrying `/EDC16C31/`.
pub fn calibration_region(data: &[u8]) -> Option<ChecksumRegion> {
    let token = data
        .windows(FAMILY_TOKEN.len())
        .position(|w| w == FAMILY_TOKEN)?;
    checksum_regions(data).into_iter().find(|r| r.contains(token))
}

fn be16(data: &[u8], off: usize) -> i16 {
    i16::from_be_bytes([data[off], data[off + 1]])
}

fn ube16(data: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([data[off], data[off + 1]])
}

fn strictly_increasing(v: &[i16]) -> bool {
    v.windows(2).all(|w| w[0] < w[1])
}

/// One axis of a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Axis {
    /// Offset of the first value.
    pub values_addr: usize,
    pub values: Vec<i16>,
}

impl Axis {
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    pub fn first(&self) -> i16 {
        self.values[0]
    }
    pub fn last(&self) -> i16 {
        *self.values.last().unwrap()
    }
}

/// One block: a 2D map (`second` is Some) or a curve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Offset of the header (`nx`).
    pub addr: usize,
    pub first: Axis,
    pub second: Option<Axis>,
    /// Offset of the first data value.
    pub data_addr: usize,
    /// Row-major data, rows = first axis, cols = second axis (or 1 col).
    pub data: Vec<i16>,
}

impl Block {
    pub fn rows(&self) -> usize {
        self.first.len()
    }
    pub fn cols(&self) -> usize {
        self.second.as_ref().map(|a| a.len()).unwrap_or(1)
    }
    pub fn is_2d(&self) -> bool {
        self.second.is_some()
    }
    /// Offset one past the last data byte.
    pub fn end(&self) -> usize {
        self.data_addr + self.data.len() * 2
    }
    pub fn row(&self, i: usize) -> &[i16] {
        let c = self.cols();
        &self.data[i * c..(i + 1) * c]
    }
    pub fn min(&self) -> i16 {
        *self.data.iter().min().unwrap()
    }
    pub fn max(&self) -> i16 {
        *self.data.iter().max().unwrap()
    }
}

/// Decode a `Kf` block at `a`, if the header and both axes are valid and
/// the block fits before `hi`.
pub fn read_kf(data: &[u8], a: usize, hi: usize) -> Option<Block> {
    if a + 8 > hi {
        return None;
    }
    let nx = ube16(data, a) as usize;
    let ny = ube16(data, a + 2) as usize;
    if !(2..=32).contains(&nx) || !(2..=32).contains(&ny) {
        return None;
    }
    let x_addr = a + 4;
    let y_addr = x_addr + 2 * nx;
    let z_addr = y_addr + 2 * ny;
    let end = z_addr + 2 * nx * ny;
    if end > hi {
        return None;
    }
    let x: Vec<i16> = (0..nx).map(|i| be16(data, x_addr + 2 * i)).collect();
    let y: Vec<i16> = (0..ny).map(|i| be16(data, y_addr + 2 * i)).collect();
    if !strictly_increasing(&x) || !strictly_increasing(&y) {
        return None;
    }
    let z: Vec<i16> = (0..nx * ny).map(|k| be16(data, z_addr + 2 * k)).collect();
    Some(Block {
        addr: a,
        first: Axis { values_addr: x_addr, values: x },
        second: Some(Axis { values_addr: y_addr, values: y }),
        data_addr: z_addr,
        data: z,
    })
}

/// Decode a `Kl` curve at `a` (3..=32 points), if its axis is valid and
/// it fits before `hi`.
pub fn read_kl(data: &[u8], a: usize, hi: usize) -> Option<Block> {
    if a + 2 > hi {
        return None;
    }
    let n = ube16(data, a) as usize;
    if !(3..=32).contains(&n) {
        return None;
    }
    let x_addr = a + 2;
    let z_addr = x_addr + 2 * n;
    if z_addr + 2 * n > hi {
        return None;
    }
    let x: Vec<i16> = (0..n).map(|i| be16(data, x_addr + 2 * i)).collect();
    if !strictly_increasing(&x) {
        return None;
    }
    let z: Vec<i16> = (0..n).map(|i| be16(data, z_addr + 2 * i)).collect();
    Some(Block { addr: a, first: Axis { values_addr: x_addr, values: x }, second: None, data_addr: z_addr, data: z })
}

/// Every `Kf` block of [lo, hi), scanned on even offsets. A decoded block
/// is consumed whole (the scan resumes after its data), so the data of a
/// map is never read as the header of another.
pub fn scan_maps(data: &[u8], lo: usize, hi: usize) -> Vec<Block> {
    let mut out = Vec::new();
    let mut a = lo + (lo & 1);
    while a + 8 <= hi {
        if let Some(b) = read_kf(data, a, hi) {
            a = b.end();
            out.push(b);
        } else {
            a += 2;
        }
    }
    out
}

/// Every `Kl` curve in the gaps between the given maps (sorted by address)
/// inside [lo, hi), consumed whole like the maps.
pub fn scan_curves(data: &[u8], maps: &[Block], lo: usize, hi: usize) -> Vec<Block> {
    let mut gaps = Vec::new();
    let mut prev = lo;
    for m in maps {
        if m.addr > prev {
            gaps.push((prev, m.addr));
        }
        prev = prev.max(m.end());
    }
    if hi > prev {
        gaps.push((prev, hi));
    }
    let mut out = Vec::new();
    for (glo, ghi) in gaps {
        let mut a = glo + (glo & 1);
        while a + 2 <= ghi {
            if let Some(b) = read_kl(data, a, ghi) {
                a = b.end();
                out.push(b);
            } else {
                a += 2;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(buf: &mut [u8], off: usize, v: i16) {
        buf[off..off + 2].copy_from_slice(&v.to_be_bytes());
    }

    #[test]
    fn kf_block_is_read_column_major_into_rows_of_the_first_axis() {
        let mut d = vec![0u8; 64];
        // nx = 2 (100, 200), ny = 3 (0, 10, 20), z[x*ny + y] = 10x + y
        let words: [i16; 2 + 2 + 3 + 6] = [2, 3, 100, 200, 0, 10, 20, 0, 1, 2, 10, 11, 12];
        for (i, w) in words.iter().enumerate() {
            put(&mut d, 2 * i, *w);
        }
        let b = read_kf(&d, 0, d.len()).expect("block");
        assert_eq!(b.rows(), 2);
        assert_eq!(b.cols(), 3);
        assert_eq!(b.row(0), &[0, 1, 2]);
        assert_eq!(b.row(1), &[10, 11, 12]);
        assert_eq!(b.first.values_addr, 4);
        assert_eq!(b.second.as_ref().unwrap().values_addr, 8);
        assert_eq!(b.data_addr, 14);
        assert_eq!(b.end(), 26);
    }

    #[test]
    fn non_increasing_axes_and_absurd_headers_are_refused() {
        let mut d = vec![0u8; 64];
        for (i, w) in [2i16, 2, 200, 100, 0, 1].iter().enumerate() {
            put(&mut d, 2 * i, *w);
        }
        assert!(read_kf(&d, 0, d.len()).is_none(), "decreasing first axis");
        assert!(read_kf(&[0xFF; 64], 0, 64).is_none(), "erased flash");
        assert!(read_kf(&[0x00; 64], 0, 64).is_none(), "zero fill");
        assert!(scan_maps(&[0xFF; 256], 0, 256).is_empty());
    }

    #[test]
    fn checksum_regions_use_the_0x800000_base() {
        let mut d = vec![0xFFu8; DUMP_SIZE];
        // region 0xE0180..0xE01BF, descriptor anywhere in the file
        d[0x1000..0x1004].copy_from_slice(&0x008E_0180u32.to_be_bytes());
        d[0x1004..0x1008].copy_from_slice(&0x008E_01BFu32.to_be_bytes());
        d[0x1008..0x1010].copy_from_slice(&CHECKSUM_MAGIC);
        let r = checksum_regions(&d);
        assert_eq!(r, vec![ChecksumRegion { descriptor: 0x1008, start: 0xE0180, end: 0xE01BF }]);
        assert!(calibration_region(&d).is_none(), "no family token inside");
        d[0xE0190..0xE0190 + FAMILY_TOKEN.len()].copy_from_slice(FAMILY_TOKEN);
        assert_eq!(calibration_region(&d).map(|r| r.start), Some(0xE0180));
        assert!(checksum_regions(&d[..0x80000]).is_empty(), "512 KB is not this family");
    }
}
