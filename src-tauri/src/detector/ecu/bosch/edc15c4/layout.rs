//! Memory layout of a Bosch EDC15C4 (BMW DDE 4.0) 512 KB dump and the
//! self-describing map records it carries.
//!
//! Confirmed on ONE real dump (BMW E39 525d, M57D25, TSW V2.40 090799
//! C4B/ESB/43, Bosch SW 1037351632):
//!
//! ```text
//! 0x00000..0x08000   C3 filler
//! 0x08000            ASCII header "TSW V2.40 090799 1418 C4B/ESB/43"
//! 0x08020..0x40000   program code (C167, little-endian)
//! 0x40000..0x50000   FF (erased)
//! 0x50000..0x60000   program code
//! 0x60000..0x70000   FF (erased)
//! 0x70000            signed calibration block, signature 67 FF FF FF FF FF FF "V2.0"
//!                    at 0x70001 (same shape as the VAG "V4.1" block, older
//!                    revision of the Bosch code-block format)
//! 0x71800..0x7B200   the calibration maps (records described below)
//! 0x7BFB0            Bosch SW number "1037351761"
//! 0x7BFFC            4-byte block checksum, block ends at 0x7C000
//! 0x7FEF0            Bosch SW number "1037351632"
//! 0x7FF00            pointer table (AA 05 ... 55 AA), C3 filler to the end
//! ```
//!
//! Every map is a **self-describing record**, the same shape as the VAG
//! EDC15 families:
//!
//! ```text
//! [id u16][n u16][n × u16 axis values, strictly increasing]   first axis
//! [id u16][m u16][m × u16 axis values, strictly increasing]   second axis (2D only)
//! [n × m × u16 data]                                         row-major, rows = FIRST axis
//! ```
//!
//! The id's high byte is in 0x80..=0xFF (0xC0/0xC1/0xC2/0xC3, 0xD9..0xDC,
//! 0x96, 0xDF on this file). Unlike the VAG EDC15P, the id does NOT name
//! the physical quantity reliably (C016 is an rpm axis on most maps, but
//! also 14..23 and 205..1100 on others), so families are recognised by
//! grid + axis value ranges + data value ranges, never by id alone.
//!
//! The data is stored with the SECOND axis as the fast index: a record
//! `[rail 15][IQ 32][data]` is 15 rows of 32 values. The app therefore
//! displays `rows = first axis` (y_axis_address) and `cols = second axis`
//! (x_axis_address), file order = display order, no transposition.

/// Signature one byte after the start of the signed calibration block:
/// `67 FF FF FF FF FF FF "V2.0"`.
pub const V20_SIGNATURE: [u8; 11] = [
    0x67, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x56, 0x32, 0x2E, 0x30,
];

/// Size of the signed block (signature -> checksum), as on the VAG EDC15P.
pub const SIGNED_BLOCK_SIZE: usize = 0xC000;

/// A 512 KB read is the only supported size (29F400 flash).
pub const DUMP_SIZE: usize = 0x80000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edc15c4Layout {
    /// Start of the signed calibration block (the byte before the signature).
    pub block_start: usize,
    /// End of the block (exclusive); the 4-byte checksum sits at end - 4.
    pub block_end: usize,
}

impl Edc15c4Layout {
    /// Locate the signed calibration block. None when the file carries no
    /// V2.0 signature: the caller must then refuse the file, never guess a
    /// range.
    pub fn detect(data: &[u8]) -> Option<Self> {
        let sig = find_v20_signature(data)?;
        let block_start = sig - 1;
        let block_end = (block_start + SIGNED_BLOCK_SIZE).min(data.len());
        Some(Self { block_start, block_end })
    }
}

/// Offset of the first V2.0 signature, if any.
pub fn find_v20_signature(data: &[u8]) -> Option<usize> {
    if data.len() < V20_SIGNATURE.len() + 1 {
        return None;
    }
    data.windows(V20_SIGNATURE.len())
        .position(|w| w == V20_SIGNATURE)
        .filter(|&p| p >= 1)
}

/// One axis of a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Axis {
    /// Offset of the `[id][n]` header.
    pub header_addr: usize,
    /// Offset of the first axis value (header_addr + 4).
    pub values_addr: usize,
    pub id: u16,
    pub values: Vec<u16>,
}

impl Axis {
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn first(&self) -> u16 {
        self.values[0]
    }
    pub fn last(&self) -> u16 {
        *self.values.last().unwrap()
    }
}

/// One self-describing record: a 1D curve (`second` is None) or a 2D map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Offset of the record (= first axis header).
    pub addr: usize,
    pub first: Axis,
    pub second: Option<Axis>,
    /// Offset of the first data value.
    pub data_addr: usize,
    /// Row-major data, rows = first axis, cols = second axis (or 1 col).
    pub data: Vec<u16>,
}

impl Record {
    pub fn rows(&self) -> usize {
        self.first.len()
    }
    pub fn cols(&self) -> usize {
        self.second.as_ref().map(|a| a.len()).unwrap_or(1)
    }
    pub fn is_2d(&self) -> bool {
        self.second.is_some()
    }
    /// Data size in bytes.
    pub fn data_size(&self) -> usize {
        self.data.len() * 2
    }
    pub fn row(&self, i: usize) -> &[u16] {
        let c = self.cols();
        &self.data[i * c..(i + 1) * c]
    }
    pub fn min(&self) -> u16 {
        self.data.iter().copied().min().unwrap_or(0)
    }
    pub fn max(&self) -> u16 {
        self.data.iter().copied().max().unwrap_or(0)
    }
}

const MIN_AXIS_LEN: usize = 2;
const MAX_AXIS_LEN: usize = 40;

#[inline]
fn rd16(data: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([data[off], data[off + 1]])
}

/// Try to read an axis header at `off`. Accepts only strictly increasing
/// values with an id whose high byte is >= 0x80 - the two properties that
/// make a record header stand out from code and from data.
pub fn axis_at(data: &[u8], off: usize, limit: usize) -> Option<Axis> {
    if off + 4 > limit {
        return None;
    }
    let id = rd16(data, off);
    let n = rd16(data, off + 2) as usize;
    if id == 0xFFFF || (id >> 8) < 0x80 || !(MIN_AXIS_LEN..=MAX_AXIS_LEN).contains(&n) {
        return None;
    }
    let values_addr = off + 4;
    if values_addr + 2 * n > limit {
        return None;
    }
    let values: Vec<u16> = (0..n).map(|i| rd16(data, values_addr + 2 * i)).collect();
    if values.windows(2).any(|w| w[0] >= w[1]) {
        return None;
    }
    Some(Axis { header_addr: off, values_addr, id, values })
}

/// Walk `[start, end)` and return every record found, in file order.
///
/// A record is consumed whole (axes + data) before the walk resumes, so a
/// data block can never be mistaken for a header of its own. The walk
/// advances two bytes at a time between records (everything is 16-bit
/// aligned in this block).
pub fn parse_records(data: &[u8], start: usize, end: usize) -> Vec<Record> {
    let end = end.min(data.len());
    let mut out = Vec::new();
    let mut off = start;
    while off + 4 <= end {
        let Some(first) = axis_at(data, off, end) else {
            off += 2;
            continue;
        };
        let after_first = first.values_addr + 2 * first.len();
        let second = axis_at(data, after_first, end);
        let (data_addr, count) = match &second {
            Some(s) => (s.values_addr + 2 * s.len(), first.len() * s.len()),
            None => (after_first, first.len()),
        };
        if data_addr + 2 * count > end {
            off += 2;
            continue;
        }
        let values: Vec<u16> = (0..count).map(|i| rd16(data, data_addr + 2 * i)).collect();
        let next = data_addr + 2 * count;
        out.push(Record { addr: off, first, second, data_addr, data: values });
        off = next;
    }
    out
}

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

    #[test]
    fn signature_is_found_one_byte_after_block_start() {
        let mut data = vec![0xC3u8; 0x80000];
        data[0x70000] = 0xF9;
        data[0x70001..0x70001 + 11].copy_from_slice(&V20_SIGNATURE);
        let layout = Edc15c4Layout::detect(&data).expect("layout");
        assert_eq!(layout.block_start, 0x70000);
        assert_eq!(layout.block_end, 0x7C000);
    }

    #[test]
    fn no_signature_means_no_layout() {
        let data = vec![0xC3u8; 0x80000];
        assert!(Edc15c4Layout::detect(&data).is_none());
    }

    #[test]
    fn parses_a_2d_record_rows_first_axis() {
        let z: Vec<u16> = (0..6).collect();
        let mut buf = vec![0u8; 16];
        buf.extend(record_2d(0xC016, &[1000, 2000, 3000], 0xC1CE, &[0, 50], &z));
        buf.extend(vec![0u8; 16]);
        let recs = parse_records(&buf, 0, buf.len());
        assert_eq!(recs.len(), 1);
        let r = &recs[0];
        assert_eq!(r.addr, 16);
        assert_eq!((r.rows(), r.cols()), (3, 2));
        assert_eq!(r.row(1), &[2, 3]);
        assert_eq!(r.data_addr, 16 + 4 + 6 + 4 + 4);
    }

    #[test]
    fn a_1d_record_has_no_second_axis() {
        let mut buf = Vec::new();
        put16(&mut buf, 0xC156);
        put16(&mut buf, 3);
        [2431u16, 2731, 3031].iter().for_each(|&v| put16(&mut buf, v));
        // data: NOT a valid axis header (id high byte < 0x80)
        [100u16, 200, 300].iter().for_each(|&v| put16(&mut buf, v));
        let recs = parse_records(&buf, 0, buf.len());
        assert_eq!(recs.len(), 1);
        assert!(!recs[0].is_2d());
        assert_eq!(recs[0].data, vec![100, 200, 300]);
    }

    #[test]
    fn non_monotonic_axis_is_not_a_record() {
        let mut buf = Vec::new();
        put16(&mut buf, 0xC016);
        put16(&mut buf, 3);
        [1000u16, 900, 3000].iter().for_each(|&v| put16(&mut buf, v));
        [1u16, 2, 3].iter().for_each(|&v| put16(&mut buf, v));
        assert!(parse_records(&buf, 0, buf.len()).is_empty());
    }
}
