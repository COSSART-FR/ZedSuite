//! Display metadata of the EDC15C4 map families: names, categories, units,
//! factors and axis labels. Every value here comes from the Bosch A2L of
//! the reference project (P079.VB4, damos 4ZB1379 / 6ZC1179), COMPU_METHOD
//! by COMPU_METHOD:
//!
//! | A2L      | unit          | raw -> physical            |
//! |----------|---------------|----------------------------|
//! | N        | 1/min         | ×1                         |
//! | MM3      | mm³/stroke    | ×0.01                      |
//! | M_L      | mg/stroke air | ×0.1                       |
//! | RP       | hPa (rail)    | ×100 hPa = ×0.1 bar        |
//! | P        | hPa           | ×1 (= mbar)                |
//! | PROZ     | %             | ×0.01                      |
//! | PROZ_S   | % (pedal)     | ×0.01                      |
//! | AD_uS    | µs            | ×1                         |
//! | GradKW   | °CA           | ×0.0234375 (signed)        |
//! | T        | °C            | ×0.1 − 273.14              |
//!
//! Quantities on this ECU are volumes (mm³/stroke), not masses: the labels
//! say so, and no conversion to mg is attempted.

use crate::models::MapCategory;

/// One axis as the user should read it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisSpec {
    pub label: &'static str,
    pub factor: f64,
    pub offset: f64,
}

pub const AX_RPM: AxisSpec = AxisSpec { label: "Engine speed (rpm)", factor: 1.0, offset: 0.0 };
pub const AX_IQ: AxisSpec = AxisSpec { label: "IQ (mm³/st)", factor: 0.01, offset: 0.0 };
pub const AX_AIR: AxisSpec = AxisSpec { label: "Airflow (mg/st)", factor: 0.1, offset: 0.0 };
pub const AX_RAIL: AxisSpec = AxisSpec { label: "Rail pressure (bar)", factor: 0.1, offset: 0.0 };
pub const AX_PEDAL: AxisSpec = AxisSpec { label: "Pedal (%)", factor: 0.01, offset: 0.0 };
pub const AX_COOLANT: AxisSpec = AxisSpec { label: "Coolant temp (degC)", factor: 0.1, offset: -273.14 };
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
    pub z_offset: f64,
    pub signed: bool,
    /// Rows = first axis of the record.
    pub y: AxisSpec,
    /// Cols = second axis (None for a curve).
    pub x: Option<AxisSpec>,
    pub confidence: f32,
}

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
    MapSpec { name, category, subcategory, description, unit, z_factor, z_offset: 0.0, signed, y, x, confidence: 0.90 }
}

// ------------------------------- Injection --------------------------------

pub const DURATION_NAMES: [&str; 6] = [
    "Injector duration 10 (no pilot)",
    "Injector duration 11 (no pilot)",
    "Injector duration 12 (no pilot)",
    "Injector duration 20 (with pilot)",
    "Injector duration 21 (with pilot)",
    "Injector duration 22 (with pilot)",
];

pub const fn duration(index: usize) -> MapSpec {
    spec(
        DURATION_NAMES[index],
        MapCategory::InjectionSystem,
        "duration",
        "Injector energising time for a requested quantity at a given rail pressure (zuwAD_KFxx) | X: IQ (mm³/st) | Y: Rail pressure (bar)",
        "µs",
        1.0,
        false,
        AX_RAIL,
        Some(AX_IQ),
    )
}

pub const RAIL_TARGET: MapSpec = spec(
    "Rail pressure target map",
    MapCategory::InjectionSystem,
    "rail",
    "Rail pressure set point by engine speed and actual quantity (zuwPQGWKF) | X: IQ (mm³/st) | Y: Engine speed (rpm)",
    "bar",
    0.1,
    false,
    AX_RPM,
    Some(AX_IQ),
);

pub const RAIL_MAX: MapSpec = spec(
    "Rail pressure maximum map",
    MapCategory::InjectionSystem,
    "rail",
    "Maximum rail pressure by engine speed and actual quantity (zuwPQmaxKF) | X: IQ (mm³/st) | Y: Engine speed (rpm)",
    "bar",
    0.1,
    false,
    AX_RPM,
    Some(AX_IQ),
);

pub const PILOT_QTY: MapSpec = spec(
    "Pilot injection quantity",
    MapCategory::InjectionSystem,
    "pilot",
    "Pilot injection quantity by engine speed and requested quantity (zuwMEVGWKF) | X: IQ (mm³/st) | Y: Engine speed (rpm)",
    "mm³/st",
    0.01,
    false,
    AX_RPM,
    Some(AX_IQ),
);

pub const PILOT_QTY_MAX: MapSpec = spec(
    "Pilot injection quantity maximum",
    MapCategory::InjectionSystem,
    "pilot",
    "Maximum pilot injection quantity (zuwMVEmxKF) | X: IQ (mm³/st) | Y: Engine speed (rpm)",
    "mm³/st",
    0.01,
    false,
    AX_RPM,
    Some(AX_IQ),
);

// ---------------------------- Start of injection ---------------------------

const SOI_FACTOR: f64 = 0.0234375;

pub const SOI_MAIN_WITH_PILOT: MapSpec = spec(
    "Main injection SOI (with pilot)",
    MapCategory::StartOfInjection,
    "main",
    "Start of main injection when a pilot injection is active (zuwABHG1KF) | X: IQ (mm³/st) | Y: Engine speed (rpm)",
    "°CA",
    SOI_FACTOR,
    true,
    AX_RPM,
    Some(AX_IQ),
);

pub const SOI_MAIN_NO_PILOT: MapSpec = spec(
    "Main injection SOI (no pilot)",
    MapCategory::StartOfInjection,
    "main",
    "Start of main injection without pilot injection (zuwABHG2KF) | X: IQ (mm³/st) | Y: Engine speed (rpm)",
    "°CA",
    SOI_FACTOR,
    true,
    AX_RPM,
    Some(AX_IQ),
);

pub const SOI_MAIN_EARLIEST: MapSpec = spec(
    "Main injection SOI earliest",
    MapCategory::StartOfInjection,
    "main",
    "Earliest allowed start of main injection (zuwABHmxKF) | X: IQ (mm³/st) | Y: Engine speed (rpm)",
    "°CA",
    SOI_FACTOR,
    true,
    AX_RPM,
    Some(AX_IQ),
);

pub const SOI_PILOT: MapSpec = spec(
    "Pilot injection SOI (relative)",
    MapCategory::StartOfInjection,
    "pilot",
    "Start of pilot injection, relative to the main injection (zuwABVGWKF) | X: IQ (mm³/st) | Y: Engine speed (rpm)",
    "°CA",
    SOI_FACTOR,
    false,
    AX_RPM,
    Some(AX_IQ),
);

// ------------------------------ Turbo boost -------------------------------

pub const BOOST_TARGET_ECO: MapSpec = spec(
    "Boost target map (eco)",
    MapCategory::TurboBoostPressure,
    "target",
    "Absolute boost pressure set point, economy program (ldwSWoekKF) | X: IQ (mm³/st) | Y: Engine speed (rpm)",
    "mbar",
    1.0,
    false,
    AX_RPM,
    Some(AX_IQ),
);

pub const BOOST_TARGET_SPORT: MapSpec = spec(
    "Boost target map (sport)",
    MapCategory::TurboBoostPressure,
    "target",
    "Absolute boost pressure set point, sport program (ldwSWspoKF) | X: IQ (mm³/st) | Y: Engine speed (rpm)",
    "mbar",
    1.0,
    false,
    AX_RPM,
    Some(AX_IQ),
);

pub const DUTY_BASE_ECO: MapSpec = spec(
    "Boost actuator duty base map (eco)",
    MapCategory::TurboBoostPressureControl,
    "duty",
    "Feed-forward duty cycle of the boost actuator, economy program (ldwTVoekKF) | X: IQ (mm³/st) | Y: Engine speed (rpm)",
    "%",
    0.01,
    false,
    AX_RPM,
    Some(AX_IQ),
);

pub const DUTY_BASE_SPORT: MapSpec = spec(
    "Boost actuator duty base map (sport)",
    MapCategory::TurboBoostPressureControl,
    "duty",
    "Feed-forward duty cycle of the boost actuator, sport program (ldwTVspoKF) | X: IQ (mm³/st) | Y: Engine speed (rpm)",
    "%",
    0.01,
    false,
    AX_RPM,
    Some(AX_IQ),
);

pub const DUTY_LIMIT_MAX: MapSpec = spec(
    "Boost actuator duty limit (max)",
    MapCategory::TurboBoostPressureControl,
    "duty",
    "Upper limit of the boost actuator duty cycle (ldwGRmaxKF) | X: IQ (mm³/st) | Y: Engine speed (rpm)",
    "%",
    0.01,
    false,
    AX_RPM,
    Some(AX_IQ),
);

pub const DUTY_LIMIT_MIN: MapSpec = spec(
    "Boost actuator duty limit (min)",
    MapCategory::TurboBoostPressureControl,
    "duty",
    "Lower limit of the boost actuator duty cycle (ldwGRminKF) | X: IQ (mm³/st) | Y: Engine speed (rpm)",
    "%",
    0.01,
    false,
    AX_RPM,
    Some(AX_IQ),
);

// ---------------------------- Smoke limitation ----------------------------

pub const SMOKE_DYNAMIC: MapSpec = spec(
    "Smoke limiter (dynamic)",
    MapCategory::SmokeLimitation,
    "smoke",
    "Quantity limit by air mass, dynamic (transient) map (mrwBRDY_KF) | X: Airflow (mg/st) | Y: Engine speed (rpm)",
    "mm³/st",
    0.01,
    false,
    AX_RPM,
    Some(AX_AIR),
);

pub const SMOKE_MAIN: MapSpec = spec(
    "Smoke limiter",
    MapCategory::SmokeLimitation,
    "smoke",
    "Quantity limit by air mass (mrwBRA_KF) | X: Airflow (mg/st) | Y: Engine speed (rpm)",
    "mm³/st",
    0.01,
    false,
    AX_RPM,
    Some(AX_AIR),
);

pub const SMOKE_LOW_RANGE: MapSpec = spec(
    "Smoke limiter (low range)",
    MapCategory::SmokeLimitation,
    "smoke",
    "Quantity limit by air mass, low range program (mrwBRLWRKF) | X: Airflow (mg/st) | Y: Engine speed (rpm)",
    "mm³/st",
    0.01,
    false,
    AX_RPM,
    Some(AX_AIR),
);

pub const SMOKE_CORR_ATM: MapSpec = spec(
    "Smoke limiter correction 1 (atmospheric pressure)",
    MapCategory::SmokeLimitation,
    "smoke",
    "First smoke limit correction, weighted by atmospheric pressure (mrwBRAkAKF) | X: Airflow (mg/st) | Y: Engine speed (rpm)",
    "mm³/st",
    0.01,
    true,
    AX_RPM,
    Some(AX_AIR),
);

pub const SMOKE_CORR_AIR_TEMP: MapSpec = spec(
    "Smoke limiter correction 2 (air temperature)",
    MapCategory::SmokeLimitation,
    "smoke",
    "Second smoke limit correction, weighted by intake air temperature (mrwBRAkLKF) | X: Airflow (mg/st) | Y: Engine speed (rpm)",
    "mm³/st",
    0.01,
    true,
    AX_RPM,
    Some(AX_AIR),
);

// ---------------------------- Torque request ------------------------------

pub const DRIVER_WISH_NAMES: [&str; 3] = [
    "Driver wish 1 (low range)",
    "Driver wish 2 (lower v_nenn)",
    "Driver wish 3 (upper v_nenn)",
];

pub const fn driver_wish(index: usize) -> MapSpec {
    spec(
        DRIVER_WISH_NAMES[index],
        MapCategory::EngineTorqueRequest,
        "driver wish",
        "Requested quantity by engine speed and pedal position (mrwFV**_KF) | X: Pedal (%) | Y: Engine speed (rpm)",
        "mm³/st",
        0.01,
        false,
        AX_RPM,
        Some(AX_PEDAL),
    )
}

// ---------------------------- Torque limiters -----------------------------

pub const TORQUE_LIMITER_NAMES: [&str; 3] = [
    "Torque limiter (pull-away)",
    "Torque limiter (raised)",
    "Torque limiter (normal)",
];

pub const fn torque_limiter(index: usize) -> MapSpec {
    spec(
        TORQUE_LIMITER_NAMES[index],
        MapCategory::EngineTorqueLimiters,
        "torque",
        "Quantity limit by engine speed (mrwADB_KL / mrwBDBH_KL / mrwBDBN_KL) | X: Engine speed (rpm)",
        "mm³/st",
        0.01,
        false,
        AX_RPM,
        None,
    )
}

pub const TORQUE_LIMITER_LOW_RANGE: MapSpec = spec(
    "Torque limiter (low range)",
    MapCategory::EngineTorqueLimiters,
    "torque",
    "Quantity limit by engine speed, low range program (mrwBDBLRKL) | X: Engine speed (rpm)",
    "mm³/st",
    0.01,
    false,
    AX_RPM,
    None,
);

pub const TURBO_PROTECTION: MapSpec = spec(
    "Turbo protection full-load quantity",
    MapCategory::EngineTorqueLimiters,
    "torque",
    "Full-load quantity limit by engine speed and atmospheric pressure, turbocharger protection (mrwLDNB_KF) | X: Atmospheric pressure (mbar) | Y: Engine speed (rpm)",
    "mm³/st",
    0.01,
    false,
    AX_RPM,
    Some(AX_ATM),
);

pub const FULL_LOAD_COOLANT: MapSpec = spec(
    "Full-load raise by coolant temperature",
    MapCategory::EngineTorqueLimiters,
    "torque",
    "Full-load quantity raise by engine speed and coolant temperature (mrwBWT_KF) | X: Coolant temp (degC) | Y: Engine speed (rpm)",
    "mm³/st",
    0.01,
    false,
    AX_RPM,
    Some(AX_COOLANT),
);

// ------------------------------ Fuel quantity -----------------------------

pub const START_QUANTITY: MapSpec = spec(
    "Start quantity base map",
    MapCategory::FuelQuantity,
    "start",
    "Cranking quantity by engine speed and coolant temperature (mrwSTMGRKF) | X: Coolant temp (degC) | Y: Engine speed (rpm)",
    "mm³/st",
    0.01,
    false,
    AX_RPM,
    Some(AX_COOLANT),
);

// ----------------------------------- EGR ----------------------------------

pub const EGR_TARGET: MapSpec = spec(
    "EGR air mass target map",
    MapCategory::Egr,
    "egr",
    "Air mass set point of the EGR control by engine speed and quantity (arwMLGRDKF) | X: IQ (mm³/st) | Y: Engine speed (rpm)",
    "mg/st",
    0.1,
    false,
    AX_RPM,
    Some(AX_IQ),
);

pub const EGR_DUTY: MapSpec = spec(
    "EGR duty base map",
    MapCategory::Egr,
    "egr",
    "Feed-forward duty cycle of the EGR valve by quantity and engine speed (arwDraTVKF) | X: Engine speed (rpm) | Y: IQ (mm³/st)",
    "%",
    0.01,
    false,
    AX_IQ,
    Some(AX_RPM),
);
