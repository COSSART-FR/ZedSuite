//! Display metadata of the EDC16C31 (BMW DDE) map families: names,
//! categories, units, factors and axis labels.
//!
//! There is no damos for the reference software in the project. The
//! conversions below come from two sources, and each one was checked on
//! the values of the reference read (E46 320d 150 hp, SW 1037361858):
//!
//! | Quantity        | raw -> physical | Source / check                                   |
//! |-----------------|-----------------|--------------------------------------------------|
//! | engine speed    | ×1 rpm          | axes 250..5020, cut-off at 5020 rpm               |
//! | quantity        | ×0.01           | Bosch EDC16CP31 A2L (same record layout); max 75..85 on the axes |
//! | torque          | ×0.1 Nm         | driver wish −45..+377 Nm, limiter peak 368 Nm     |
//! | pedal           | ×100/8192 %     | axis ends at exactly 8192 (Bosch `Prc`)           |
//! | duty cycle      | ×100/8192 %     | N75 base maps 15..83 %                            |
//! | rail pressure   | ×0.1 bar        | set points 300..1605 bar, SVRL 1600 bar           |
//! | boost pressure  | ×1 hPa (mbar)   | 990 hPa at idle, 2400 hPa at full load            |
//! | injection angle | ×3/128 °CA      | Bosch EDC16 `GradKW`, 2..32 °CA before TDC        |
//! | energising time | ×1 µs           | 1211 µs for 85 at 1600 bar, 5000 µs cap           |
//!
//! The quantity unit is labelled mm³/stroke as in the Bosch A2Ls of the
//! two related projects (DDE 4.0 and the Mercedes CP31). It is NOT
//! confirmed on a BMW EDC16 damos; see docs/PORTING-EDC16C31.md.

use crate::models::MapCategory;

/// One axis as the user should read it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisSpec {
    pub label: &'static str,
    pub factor: f64,
    pub offset: f64,
}

const PRC: f64 = 100.0 / 8192.0;

pub const AX_RPM: AxisSpec = AxisSpec { label: "Engine speed (rpm)", factor: 1.0, offset: 0.0 };
pub const AX_IQ: AxisSpec = AxisSpec { label: "IQ (mm³/st)", factor: 0.01, offset: 0.0 };
pub const AX_TORQUE: AxisSpec = AxisSpec { label: "Torque (Nm)", factor: 0.1, offset: 0.0 };
pub const AX_PEDAL: AxisSpec = AxisSpec { label: "Pedal (%)", factor: PRC, offset: 0.0 };
pub const AX_RAIL: AxisSpec = AxisSpec { label: "Rail pressure (bar)", factor: 0.1, offset: 0.0 };
pub const AX_RAIL_SP: AxisSpec = AxisSpec { label: "Rail pressure set point (bar)", factor: 0.1, offset: 0.0 };
pub const AX_BOOST: AxisSpec = AxisSpec { label: "Boost pressure (mbar)", factor: 1.0, offset: 0.0 };
pub const AX_ATM: AxisSpec = AxisSpec { label: "Atmospheric pressure (mbar)", factor: 1.0, offset: 0.0 };

/// Display spec of one map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapSpec {
    pub name: &'static str,
    pub category: MapCategory,
    pub subcategory: &'static str,
    pub description: &'static str,
    pub unit: &'static str,
    pub z_factor: f64,
    pub signed: bool,
    /// Rows = first axis of the block.
    pub y: AxisSpec,
    /// Cols = second axis (None for a curve).
    pub x: Option<AxisSpec>,
    pub confidence: f32,
}

#[allow(clippy::too_many_arguments)]
const fn spec(
    name: &'static str,
    category: MapCategory,
    subcategory: &'static str,
    description: &'static str,
    unit: &'static str,
    z_factor: f64,
    signed: bool,
    y: AxisSpec,
    x: Option<AxisSpec>,
) -> MapSpec {
    MapSpec { name, category, subcategory, description, unit, z_factor, signed, y, x, confidence: 0.85 }
}

// ---------------------------- Torque request ------------------------------

pub const DRIVER_WISH_NAMES: [&str; 2] = ["Driver wish 1", "Driver wish 2"];

pub const fn driver_wish(i: usize) -> MapSpec {
    spec(
        DRIVER_WISH_NAMES[i],
        MapCategory::EngineTorqueRequest,
        "driver wish",
        "Requested engine torque by engine speed and pedal position (negative = overrun) | X: Pedal (%) | Y: Engine speed (rpm)",
        "Nm",
        0.1,
        true,
        AX_RPM,
        Some(AX_PEDAL),
    )
}

// ---------------------------- Torque limiters -----------------------------

pub const TORQUE_LIMITER: MapSpec = spec(
    "Torque limiter",
    MapCategory::EngineTorqueLimiters,
    "torque",
    "Engine torque limit by engine speed, falling to zero at the cut-off speed | X: Engine speed (rpm)",
    "Nm",
    0.1,
    false,
    AX_RPM,
    None,
);

pub const TORQUE_LIMITER_SET_NAMES: [&str; 5] = [
    "Torque limiter 1",
    "Torque limiter 2",
    "Torque limiter 3",
    "Torque limiter 4",
    "Torque limiter 5",
];

pub const fn torque_limiter_set(i: usize) -> MapSpec {
    spec(
        TORQUE_LIMITER_SET_NAMES[i],
        MapCategory::EngineTorqueLimiters,
        "torque",
        "Engine torque limit by engine speed, one of a contiguous set of five curves (gear-dependent according to public map lists, not verified) | X: Engine speed (rpm)",
        "Nm",
        0.1,
        false,
        AX_RPM,
        None,
    )
}

// ------------------------------ Fuel quantity -----------------------------

pub const TORQUE_TO_IQ_NAMES: [&str; 3] = ["Torque to IQ 1", "Torque to IQ 2", "Torque to IQ 3"];

pub const fn torque_to_iq(i: usize) -> MapSpec {
    spec(
        TORQUE_TO_IQ_NAMES[i],
        MapCategory::FuelQuantity,
        "conversion",
        "Injection quantity needed for a requested torque at a given engine speed | X: Torque (Nm) | Y: Engine speed (rpm)",
        "mm³/st",
        0.01,
        false,
        AX_RPM,
        Some(AX_TORQUE),
    )
}

// ---------------------------- Smoke limitation ----------------------------

pub const SMOKE_BOOST: MapSpec = spec(
    "Smoke limiter (boost pressure)",
    MapCategory::SmokeLimitation,
    "smoke",
    "Quantity limit by boost pressure and engine speed | X: Engine speed (rpm) | Y: Boost pressure (mbar)",
    "mm³/st",
    0.01,
    false,
    AX_BOOST,
    Some(AX_RPM),
);

// ------------------------------- Injection --------------------------------

pub const DURATION_NAMES: [&str; 6] = [
    "Injector duration 1",
    "Injector duration 2",
    "Injector duration 3",
    "Injector duration 4",
    "Injector duration 5",
    "Injector duration 6",
];

pub const fn duration(i: usize) -> MapSpec {
    spec(
        DURATION_NAMES[i],
        MapCategory::InjectionSystem,
        "duration",
        "Injector energising time for a requested quantity at a given rail pressure | X: IQ (mm³/st) | Y: Rail pressure (bar)",
        "µs",
        1.0,
        false,
        AX_RAIL,
        Some(AX_IQ),
    )
}

pub const RAIL_TARGET_NAMES: [&str; 2] = ["Rail pressure target map 1", "Rail pressure target map 2"];

pub const fn rail_target(i: usize) -> MapSpec {
    spec(
        RAIL_TARGET_NAMES[i],
        MapCategory::InjectionSystem,
        "rail",
        "Rail pressure set point by engine speed and quantity | X: IQ (mm³/st) | Y: Engine speed (rpm)",
        "bar",
        0.1,
        false,
        AX_RPM,
        Some(AX_IQ),
    )
}

pub const RAIL_LIMITER_NAMES: [&str; 2] = ["Rail pressure limiter 1", "Rail pressure limiter 2"];

pub const fn rail_limiter(i: usize) -> MapSpec {
    spec(
        RAIL_LIMITER_NAMES[i],
        MapCategory::InjectionSystem,
        "rail",
        "Rail pressure set point after limitation, by engine speed and requested set point | X: Rail pressure set point (bar) | Y: Engine speed (rpm)",
        "bar",
        0.1,
        false,
        AX_RPM,
        Some(AX_RAIL_SP),
    )
}

// ---------------------------- Start of injection ---------------------------

const SOI_FACTOR: f64 = 0.0234375;

pub const SOI_NAMES: [&str; 5] = [
    "Main injection SOI 1",
    "Main injection SOI 2",
    "Main injection SOI 3",
    "Main injection SOI 4",
    "Main injection SOI 5",
];

pub const fn soi(i: usize) -> MapSpec {
    spec(
        SOI_NAMES[i],
        MapCategory::StartOfInjection,
        "main",
        "Start of main injection (°CA before TDC) by engine speed and quantity | X: IQ (mm³/st) | Y: Engine speed (rpm)",
        "°CA",
        SOI_FACTOR,
        true,
        AX_RPM,
        Some(AX_IQ),
    )
}

// ------------------------------ Turbo boost -------------------------------

pub const BOOST_TARGET_NAMES: [&str; 3] = ["Boost target map 1", "Boost target map 2", "Boost target map 3"];

pub const fn boost_target(i: usize) -> MapSpec {
    spec(
        BOOST_TARGET_NAMES[i],
        MapCategory::TurboBoostPressure,
        "target",
        "Absolute boost pressure set point by engine speed and quantity | X: IQ (mm³/st) | Y: Engine speed (rpm)",
        "mbar",
        1.0,
        false,
        AX_RPM,
        Some(AX_IQ),
    )
}

pub const BOOST_LIMITER_ATM: MapSpec = spec(
    "Boost limiter (atmospheric pressure)",
    MapCategory::TurboBoostPressure,
    "limiter",
    "Absolute boost pressure limit by engine speed and atmospheric pressure | X: Atmospheric pressure (mbar) | Y: Engine speed (rpm)",
    "mbar",
    1.0,
    false,
    AX_RPM,
    Some(AX_ATM),
);

pub const DUTY_NAMES: [&str; 5] = [
    "Boost actuator duty map 1",
    "Boost actuator duty map 2",
    "Boost actuator duty map 3",
    "Boost actuator duty map 4",
    "Boost actuator duty map 5",
];

pub const fn duty(i: usize) -> MapSpec {
    spec(
        DUTY_NAMES[i],
        MapCategory::TurboBoostPressureControl,
        "duty",
        "Feed-forward duty cycle of the turbo actuator by engine speed and quantity | X: IQ (mm³/st) | Y: Engine speed (rpm)",
        "%",
        PRC,
        false,
        AX_RPM,
        Some(AX_IQ),
    )
}
