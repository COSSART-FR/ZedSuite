//! Classification of the EDC16C31 blocks (see layout.rs) into map families.
//!
//! Every rule reads the BLOCK: grid, axis value ranges, data value ranges
//! and, for families that come as a set, how many blocks answer and
//! whether they are contiguous. No rule reads an address.
//!
//! A family is only reported when the file answers with the expected
//! number of blocks: two driver wish maps, three torque-to-IQ maps, five
//! SOI maps in a row, six injector durations in a row, three boost
//! targets, five duty maps, two rail targets, two rail limiters, five
//! contiguous torque limiter curves, and exactly one smoke limiter, one
//! boost limiter and one main torque limiter. Anything else means the rule
//! does not recognise this software, and the family is dropped whole
//! rather than guessed ("fewer maps that are right", CONTRIBUTING.md).

use super::layout::{Axis, Block};
use super::spec::{self, MapSpec};

/// A block with the spec it was recognised as.
#[derive(Debug, Clone)]
pub struct Classified {
    pub block: Block,
    pub spec: MapSpec,
}

/// Data ceilings. The app detects every file in stock mode, tuned ones
/// included, so the stock ceilings already leave room for a usual remap
/// (boost 4000 hPa, rail 2000 bar, 800 Nm limiters): what tells a family
/// apart here is its axes and its set, not its values. Tuned mode widens a
/// little more. Axis rules never widen.
#[derive(Debug, Clone, Copy)]
struct Limits {
    tuned: bool,
}

impl Limits {
    fn pick(&self, stock: i16, tuned: i16) -> i16 {
        if self.tuned { tuned } else { stock }
    }
}

// ----------------------------- axis shapes --------------------------------

/// Engine speed: 8..21 points from 0..1000 rpm up to 3500..6000 rpm.
fn is_rpm(a: &Axis) -> bool {
    (8..=21).contains(&a.len()) && (0..=1000).contains(&a.first()) && (3500..=6000).contains(&a.last())
}

/// Engine speed up to the full-load range (4500 rpm and more).
fn is_rpm_full(a: &Axis) -> bool {
    is_rpm(a) && a.last() >= 4500
}

/// Quantity (×0.01): from 0 up to 50..90.
fn is_iq(a: &Axis) -> bool {
    a.first() == 0 && (5000..=9000).contains(&a.last())
}

/// Torque (×0.1 Nm): from 0 up to 300..500 Nm.
fn is_torque(a: &Axis) -> bool {
    a.first() == 0 && (3000..=5000).contains(&a.last())
}

/// Pedal (1/8192): exactly 0 to 8192.
fn is_pedal(a: &Axis) -> bool {
    a.first() == 0 && a.last() == 8192
}

/// Rail pressure (×0.1 bar): 100..200 bar first, 1200..1800 bar last.
fn is_rail(a: &Axis) -> bool {
    (10..=20).contains(&a.len()) && (1000..=2000).contains(&a.first()) && (12_000..=18_000).contains(&a.last())
}

/// Rail pressure set point (×0.1 bar): from 0 up to 1200..1800 bar.
fn is_rail_set_point(a: &Axis) -> bool {
    a.len() >= 10 && a.first() == 0 && (12_000..=18_000).contains(&a.last())
}

/// Boost pressure (hPa): 500..1200 first, 2000..3500 last.
fn is_boost_pressure(a: &Axis) -> bool {
    (500..=1200).contains(&a.first()) && (2000..=3500).contains(&a.last())
}

/// Atmospheric pressure (hPa): every point in 500..1100.
fn is_atmospheric(a: &Axis) -> bool {
    a.values.iter().all(|&v| (500..=1100).contains(&v))
}

fn second(b: &Block) -> Option<&Axis> {
    b.second.as_ref()
}

fn rows_non_decreasing(b: &Block) -> bool {
    (0..b.rows()).all(|i| b.row(i).windows(2).all(|w| w[0] <= w[1]))
}

// ------------------------------- map rules --------------------------------

fn is_driver_wish(b: &Block, l: Limits) -> bool {
    let Some(p) = second(b) else { return false };
    is_rpm(&b.first) && is_pedal(p) && b.min() < 0 && (2000..=l.pick(9000, 12_000)).contains(&b.max())
}

fn is_smoke_boost(b: &Block, l: Limits) -> bool {
    let Some(n) = second(b) else { return false };
    is_boost_pressure(&b.first)
        && is_rpm(n)
        && b.min() >= 1000
        && (5000..=l.pick(20_000, 25_000)).contains(&b.max())
        // a flat block is an unused program, not the limiter
        && b.max() - b.min() >= 1000
}

fn is_torque_to_iq(b: &Block, l: Limits) -> bool {
    let Some(t) = second(b) else { return false };
    is_rpm(&b.first)
        && is_torque(t)
        && b.rows() == 16
        && b.cols() == 16
        && (0..b.rows()).all(|i| b.row(i)[0] == 0)
        && rows_non_decreasing(b)
        && (5000..=l.pick(25_000, 30_000)).contains(&b.max())
}

fn is_soi(b: &Block, l: Limits) -> bool {
    let Some(q) = second(b) else { return false };
    // 15..40 °CA advance at most, no more than 21 °CA after TDC
    is_rpm_full(&b.first)
        && is_iq(q)
        && b.rows() == 16
        && b.cols() == 16
        && b.min() >= -900
        && (640..=l.pick(2200, 2600)).contains(&b.max())
}

fn is_duration(b: &Block, l: Limits) -> bool {
    let Some(q) = second(b) else { return false };
    is_rail(&b.first)
        && (24..=40).contains(&q.len())
        && is_iq(q)
        && b.min() >= 0
        && b.max() <= l.pick(10_000, 12_000)
        && (0..b.rows()).all(|i| b.row(i)[0] == 0)
        && rows_non_decreasing(b)
}

fn is_boost_target(b: &Block, l: Limits) -> bool {
    let Some(q) = second(b) else { return false };
    is_rpm_full(&b.first)
        && is_iq(q)
        && b.min() >= 800
        && (1500..=l.pick(4000, 4500)).contains(&b.max())
        && (850..=1150).contains(&b.row(0)[0])
}

fn is_boost_limiter_atm(b: &Block, l: Limits) -> bool {
    let Some(p) = second(b) else { return false };
    is_rpm(&b.first) && is_atmospheric(p) && b.min() >= 800 && (1500..=l.pick(4000, 4500)).contains(&b.max())
}

fn is_rail_target(b: &Block, l: Limits) -> bool {
    let Some(q) = second(b) else { return false };
    is_rpm_full(&b.first)
        && is_iq(q)
        && (2000..=5000).contains(&b.min())
        && (12_000..=l.pick(20_000, 22_000)).contains(&b.max())
}

fn is_rail_limiter(b: &Block, l: Limits) -> bool {
    let Some(sp) = second(b) else { return false };
    is_rpm(&b.first)
        && is_rail_set_point(sp)
        && b.min() >= 0
        && (12_000..=l.pick(20_000, 22_000)).contains(&b.max())
        && rows_non_decreasing(b)
}

fn is_duty(b: &Block, _l: Limits) -> bool {
    let Some(q) = second(b) else { return false };
    // 12..100 %, never flat (a flat 8192 block is a disabled program)
    is_rpm_full(&b.first)
        && is_iq(q)
        && b.rows() == 16
        && b.min() >= 1000
        && (5000..=8192).contains(&b.max())
        && b.max() - b.min() >= 500
}

// ------------------------------ curve rules -------------------------------

fn is_torque_curve(b: &Block, l: Limits) -> bool {
    !b.is_2d()
        && is_rpm(&b.first)
        && b.rows() >= 15
        && b.first.last() >= 4500
        && b.min() >= 0
        && (2500..=l.pick(8000, 10_000)).contains(&b.max())
}

/// The main limiter falls to (almost) nothing at the cut-off speed; the
/// cut-off sits at 4800..5300 rpm.
fn is_main_torque_limiter(b: &Block, l: Limits) -> bool {
    is_torque_curve(b, l)
        && (4800..=5300).contains(&b.first.last())
        && (*b.data.last().unwrap() as i32) * 4 <= b.max() as i32
}

// ------------------------------ assembly ----------------------------------

/// Indices of the blocks matching `rule`.
fn matching(blocks: &[Block], l: Limits, rule: fn(&Block, Limits) -> bool) -> Vec<usize> {
    (0..blocks.len()).filter(|&i| rule(&blocks[i], l)).collect()
}

/// Exactly `n` matches, or nothing.
fn exactly(blocks: &[Block], l: Limits, n: usize, rule: fn(&Block, Limits) -> bool) -> Option<Vec<usize>> {
    let m = matching(blocks, l, rule);
    (m.len() == n).then_some(m)
}

/// The single run of exactly `n` consecutive matching blocks, where
/// consecutive means adjacent in `blocks` and, when `max_gap` is set, no
/// more than `max_gap` bytes between one block's end and the next start.
fn single_run(
    blocks: &[Block],
    l: Limits,
    n: usize,
    max_gap: Option<usize>,
    rule: fn(&Block, Limits) -> bool,
) -> Option<Vec<usize>> {
    let mut runs: Vec<Vec<usize>> = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    for i in 0..blocks.len() {
        if !rule(&blocks[i], l) {
            if !cur.is_empty() {
                runs.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if let (Some(&prev), Some(gap)) = (cur.last(), max_gap) {
            let prev: &Block = &blocks[prev];
            if blocks[i].addr < prev.end() || blocks[i].addr - prev.end() > gap {
                runs.push(std::mem::take(&mut cur));
            }
        }
        cur.push(i);
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    let mut exact = runs.into_iter().filter(|r| r.len() == n);
    let first = exact.next()?;
    if exact.next().is_some() {
        return None;
    }
    Some(first)
}

/// Name the maps and curves of the calibration region.
pub fn classify(maps: &[Block], curves: &[Block], tuned: bool) -> Vec<Classified> {
    let l = Limits { tuned };
    let mut out = Vec::new();
    let mut push = |blocks: &[Block], idx: Option<Vec<usize>>, name: &dyn Fn(usize) -> MapSpec| {
        if let Some(idx) = idx {
            for (k, i) in idx.into_iter().enumerate() {
                out.push(Classified { block: blocks[i].clone(), spec: name(k) });
            }
        }
    };

    push(maps, exactly(maps, l, 2, is_driver_wish), &spec::driver_wish);
    push(maps, exactly(maps, l, 1, is_smoke_boost), &|_| spec::SMOKE_BOOST);
    push(maps, exactly(maps, l, 3, is_torque_to_iq), &spec::torque_to_iq);
    push(maps, single_run(maps, l, 5, None, is_soi), &spec::soi);
    push(maps, single_run(maps, l, 6, None, is_duration), &spec::duration);
    push(maps, exactly(maps, l, 3, is_boost_target), &spec::boost_target);
    push(maps, exactly(maps, l, 1, is_boost_limiter_atm), &|_| spec::BOOST_LIMITER_ATM);
    push(maps, exactly(maps, l, 2, is_rail_target), &spec::rail_target);
    push(maps, exactly(maps, l, 2, is_rail_limiter), &spec::rail_limiter);
    push(maps, exactly(maps, l, 5, is_duty), &spec::duty);

    // Curves: the contiguous set of five first (a 19-point curve in a
    // 21-point slot leaves 8 bytes of padding, hence the gap), then the
    // main limiter among the curves left.
    let set = single_run(curves, l, 5, Some(16), is_torque_curve);
    let in_set: Vec<usize> = set.clone().unwrap_or_default();
    let main: Vec<usize> = (0..curves.len())
        .filter(|i| !in_set.contains(i) && is_main_torque_limiter(&curves[*i], l))
        .collect();
    push(curves, set, &spec::torque_limiter_set);
    push(curves, (main.len() == 1).then_some(main), &|_| spec::TORQUE_LIMITER);

    out.sort_by_key(|c| c.block.addr);
    out
}
