//! The tank, the engine and the propeller.
//!
//! A piston engine turns one propeller and starts from rest, which is safe
//! because nothing but the starter drives a stopped shaft. An electric motor
//! has no such start: JSBSim charges a stopped propeller `power / 0.01` of
//! torque on its first step, so the stand-in splits its power between two
//! coincident propellers of opposite sense, whose torques and gyroscopics
//! cancel. A real aeroplane rigs its torque out, and a conceptual design has no
//! rigging to read, so that stand-in models no torque effect at all, delivers
//! the same shaft power at any altitude and burns nothing: its fuel stays
//! aboard as mass.

use std::f64::consts::PI;
use std::fmt::Write;

use super::{INCHES_PER_METRE, POUNDS_PER_KG, location, nose_station_m};
use crate::{AceExportError, FixedWingDesign, PistonDesign, Powerplant};

/// Sea-level speed of sound, m/s: the propeller's tip limit is a design limit
/// and does not move with the cruise altitude.
pub(super) const SPEED_OF_SOUND_M_S: f64 = 340.3;
/// Share of the tip-Mach limit the rated speed of an electric motor uses,
/// leaving the forward speed room in the helical tip speed.
const TIP_MARGIN: f64 = 0.8;
/// Propeller spin inertia: a metal two-blade on a light aeroplane, slug·ft².
const PROPELLER_INERTIA_SLUG_FT2: f64 = 2.0;
/// Coincident propellers sharing an electric motor's shaft power.
pub(super) const ELECTRIC_PROPELLERS: f64 = 2.0;
/// ISA sea-level density, kg/m³: the density a piston engine is rated at.
const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;
const WATTS_PER_HP: f64 = 745.7;
/// Cubic inches per horsepower of a naturally aspirated flat four: the
/// catalog gives no displacement, and the Lycoming O-360 is 360 at 180 hp.
const DISPLACEMENT_IN3_PER_HP: f64 = 2.0;
/// Manifold pressures of an unboosted engine at closed and open throttle,
/// inHg.
const MANIFOLD_INHG: (f64, f64) = (10.0, 28.5);
/// Slowest speed the engine runs at, rpm.
const IDLE_RPM: f64 = 600.0;
/// Share of the power one magneto loses against both.
const SPARK_FAIL_DROP: f64 = 0.1;

/// The second engine of the electric pair takes its throttle here: the flight
/// controls' own throttle property reaches only the first.
pub(super) const THROTTLE_LINK: &str = "    <channel name=\"Throttle\">
      <pure_gain name=\"fcs/second-throttle\">
        <input>fcs/throttle-cmd-norm</input>
        <gain>1</gain>
        <output>fcs/throttle-pos-norm[1]</output>
      </pure_gain>
    </channel>
";

/// A fixed-pitch propeller curve anchored at the cruise point.
pub(super) struct PropellerCurve {
    pub(super) rated_rev_s: f64,
    pub(super) cruise_advance: f64,
    pub(super) cruise_ct: f64,
    pub(super) cruise_cp: f64,
    /// The shaft power the curve absorbs at the cruise point, W.
    pub(super) cruise_power_w: f64,
}

impl PropellerCurve {
    /// A propeller that absorbs each shaft's power at cruise density at the
    /// rated speed: the throttle stays open at the top of its range, and a
    /// piston engine's power has lapsed with the density by then.
    pub(super) fn of(design: &FixedWingDesign) -> Result<Self, AceExportError> {
        let propeller = design.propeller;
        let diameter = propeller.diameter_m;
        let density = design.cruise_density_kg_m3()?;
        let (rated_rev_s, cruise_power_w) = match design.powerplant {
            Powerplant::Piston(PistonDesign {
                rated_rpm,
                power_lapse_exponent,
            }) => {
                let lapse = (density / SEA_LEVEL_DENSITY_KG_M3).powf(power_lapse_exponent);
                (rated_rpm / 60.0, design.shaft_power_w * lapse)
            }
            Powerplant::Electric => {
                let tip_m_s = propeller.tip_mach_limit * SPEED_OF_SOUND_M_S * TIP_MARGIN;
                (
                    tip_m_s / (PI * diameter),
                    design.shaft_power_w / ELECTRIC_PROPELLERS,
                )
            }
        };
        let cruise_cp = cruise_power_w / (density * rated_rev_s.powi(3) * diameter.powi(5));
        let cruise_advance = design.cruise_speed_m_s / (rated_rev_s * diameter);
        Ok(Self {
            rated_rev_s,
            cruise_advance,
            cruise_ct: propeller.cruise_efficiency * cruise_cp / cruise_advance,
            cruise_cp,
            cruise_power_w,
        })
    }

    /// `C(J)` rows at multiples of the cruise advance ratio, from a stalled
    /// blade at rest out to the windmilling side.
    fn table(&self, cruise: f64, shape: [f64; 5]) -> String {
        let mut rows = String::new();
        let advance = [0.0, 0.5, 1.0, 1.5, 2.0].map(|share| share * self.cruise_advance);
        for (advance, factor) in advance.into_iter().zip(shape) {
            let value = cruise * factor;
            // Writing to a `String` cannot fail.
            writeln!(rows, "            {advance:.5} {value:.6}").ok();
        }
        rows
    }
}

fn piston_engine(design: &FixedWingDesign, piston: PistonDesign) -> String {
    let horsepower = design.shaft_power_w / WATTS_PER_HP;
    let (min_manifold, max_manifold) = MANIFOLD_INHG;
    format!(
        "      <piston_engine name=\"piston\">
        <minmp unit=\"INHG\"> {min_manifold:.1} </minmp>
        <maxmp unit=\"INHG\"> {max_manifold:.1} </maxmp>
        <displacement unit=\"IN3\"> {displacement:.1} </displacement>
        <maxhp> {horsepower:.2} </maxhp>
        <cycles> 4 </cycles>
        <idlerpm> {IDLE_RPM:.0} </idlerpm>
        <maxrpm> {rpm:.0} </maxrpm>
        <maxthrottle> 1 </maxthrottle>
        <minthrottle> 0.1 </minthrottle>
        <sparkfaildrop> {SPARK_FAIL_DROP} </sparkfaildrop>
      </piston_engine>
",
        displacement = horsepower * DISPLACEMENT_IN3_PER_HP,
        rpm = piston.rated_rpm,
    )
}

fn electric_engine(curve: &PropellerCurve) -> String {
    format!(
        "      <electric_engine name=\"shaft\">
        <power unit=\"WATTS\"> {power:.1} </power>
      </electric_engine>
",
        power = curve.cruise_power_w,
    )
}

/// `<engine>` driving one propeller of rotation `sense`.
fn engine(design: &FixedWingDesign, curve: &PropellerCurve, motor: &str, sense: i8) -> String {
    let propeller = design.propeller;
    format!(
        "    <engine>
      <feed> 0 </feed>
{motor}      <thruster>
        {nose}
        <sense> {sense} </sense>
        <propeller name=\"prop\">
          <ixx> {PROPELLER_INERTIA_SLUG_FT2:.1} </ixx>
          <diameter unit=\"IN\"> {diameter:.3} </diameter>
          <numblades> {blades} </numblades>
          <gearratio> 1 </gearratio>
          <table name=\"C_THRUST\" type=\"internal\">
            <tableData>
{ct}            </tableData>
          </table>
          <table name=\"C_POWER\" type=\"internal\">
            <tableData>
{cp}            </tableData>
          </table>
        </propeller>
      </thruster>
    </engine>
",
        nose = location(None, nose_station_m(design)),
        diameter = propeller.diameter_m * INCHES_PER_METRE,
        blades = propeller.blade_count,
        ct = curve.table(curve.cruise_ct, [1.6, 1.3, 1.0, 0.0, -0.5]),
        cp = curve.table(curve.cruise_cp, [1.2, 1.1, 1.0, 0.6, 0.2]),
    )
}

pub(super) fn propulsion(design: &FixedWingDesign, curve: &PropellerCurve) -> String {
    let engines = match design.powerplant {
        Powerplant::Piston(piston) => engine(design, curve, &piston_engine(design, piston), 1),
        Powerplant::Electric => {
            let motor = electric_engine(curve);
            engine(design, curve, &motor, 1) + &engine(design, curve, &motor, -1)
        }
    };
    format!(
        "  <propulsion>
    <tank type=\"FUEL\">
      {tank_at}
      <capacity unit=\"LBS\"> {capacity:.4} </capacity>
      <contents unit=\"LBS\"> {contents:.4} </contents>
    </tank>
{engines}  </propulsion>
  <!-- Rated at {rpm:.0} rpm: cruise J {j:.3}, CT {cruise_ct:.4}, CP {cruise_cp:.4}. -->",
        tank_at = location(None, design.fuel.station_m),
        capacity = design.fuel_capacity_kg * POUNDS_PER_KG,
        contents = design.fuel.mass_kg * POUNDS_PER_KG,
        rpm = curve.rated_rev_s * 60.0,
        j = curve.cruise_advance,
        cruise_ct = curve.cruise_ct,
        cruise_cp = curve.cruise_cp,
    )
}
