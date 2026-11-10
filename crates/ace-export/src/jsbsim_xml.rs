use glam::DVec3;

use crate::{AceExportError, FixedWingDesign, Powerplant};

mod aero;
mod ground;
mod propulsion;

use propulsion::PropellerCurve;

#[cfg(test)]
mod tests;

pub(crate) const FEET_PER_METRE: f64 = 1.0 / 0.3048;
pub(crate) const INCHES_PER_METRE: f64 = FEET_PER_METRE * 12.0;
pub(crate) const POUNDS_PER_KG: f64 = 2.204_622_621_8;
/// Kilogram square metres in one slug square foot.
const KG_M2_PER_SLUG_FT2: f64 = 1.355_817_962;
/// How far the thrust line sits ahead of the empty centre of gravity, metres.
pub(crate) const NOSE_ARM_M: f64 = 2.0;

/// The nose, in the model's axes: where the propeller and the forward camera
/// sit. A conceptual design has no fuselage to read it from.
pub(crate) fn nose_station_m(design: &FixedWingDesign) -> DVec3 {
    design.empty.station_m - DVec3::X * NOSE_ARM_M
}

/// A JSBSim aircraft document for `design`, named `model_name`.
///
/// Forces and moments act about the loaded centre of gravity, with `Cm0` set
/// so the aircraft trims at [`FixedWingDesign::cruise_alpha_rad`] with the
/// elevator neutral. The pitch channel sums `fcs/pitch-trim-cmd-norm` into the
/// elevator, so a trim survives the pilot centring the stick.
///
/// A [`Powerplant::Piston`](crate::Powerplant::Piston) design gets a real
/// `<piston_engine>` that burns the tank down and turns one propeller, whose
/// torque and gyroscopics act on the airframe: it starts stopped, on its
/// magnetos and starter. Any other design gets the electric stand-in described
/// in `jsbsim_xml/propulsion.rs`.
///
/// Propeller coefficients are built around the cruise point, so that at the
/// rated speed the design's cruise efficiency holds at the cruise advance
/// ratio. They are a fixed-pitch curve, not blade data, whatever the catalog
/// says of the blades.
///
/// The tank holds the design's fuel at the fuel station, and the empty weight
/// and payload point mass carry the rest, so the aircraft's mass, centre of
/// gravity and inertia are the component tree's. `<contents>` is
/// [`FixedWingDesign::fuel`]'s mass.
pub fn jsbsim_aircraft_xml(
    design: &FixedWingDesign,
    model_name: &str,
) -> Result<String, AceExportError> {
    let mut xml = String::new();
    let cruise_alpha = design.cruise_alpha_rad()?;
    let propeller = PropellerCurve::of(design)?;
    let links = match design.powerplant {
        Powerplant::Piston(_) => "",
        Powerplant::Electric => propulsion::THROTTLE_LINK,
    };
    let lines = [
        "<?xml version=\"1.0\"?>".to_owned(),
        format!("<!-- Compiled by ace-export from {}. -->", design.id),
        format!("<fdm_config name=\"{model_name}\" version=\"2.0\" release=\"ALPHA\">"),
        metrics(design),
        mass_balance(design),
        ground::ground_reactions(design),
        propulsion::propulsion(design, &propeller),
        aero::flight_control(links),
        aero::aerodynamics(design, cruise_alpha),
        "</fdm_config>\n".to_owned(),
    ];
    for line in lines {
        write_line(&mut xml, &line);
    }
    Ok(xml)
}

fn write_line(xml: &mut String, line: &str) {
    xml.push_str(line);
    xml.push('\n');
}

/// `<location>` of `station_m`, which JSBSim's structural frame shares: `x`
/// aft, `y` right, `z` up.
fn location(name: Option<&str>, station_m: DVec3) -> String {
    let DVec3 { x, y, z } = station_m * INCHES_PER_METRE;
    let label = name.map_or_else(String::new, |name| format!(" name=\"{name}\""));
    format!(
        "<location{label} unit=\"IN\"> <x> {x:.4} </x> <y> {y:.4} </y> <z> {z:.4} </z> </location>"
    )
}

fn metrics(design: &FixedWingDesign) -> String {
    let cg = design.loaded_cg_station_m();
    let chord_ft = design.mean_chord_m() * FEET_PER_METRE;
    let area_ft2 = design.wing_area_m2 * FEET_PER_METRE * FEET_PER_METRE;
    let span_ft = design.wing_span_m * FEET_PER_METRE;
    format!(
        "  <metrics>
    <wingarea unit=\"FT2\"> {area_ft2:.4} </wingarea>
    <wingspan unit=\"FT\"> {span_ft:.4} </wingspan>
    <chord unit=\"FT\"> {chord_ft:.4} </chord>
    <htailarea unit=\"FT2\"> 0 </htailarea>
    <htailarm unit=\"FT\"> 0 </htailarm>
    <vtailarea unit=\"FT2\"> 0 </vtailarea>
    <vtailarm unit=\"FT\"> 0 </vtailarm>
    {aerorp}
    {eyepoint}
    {vrp}
  </metrics>",
        aerorp = location(Some("AERORP"), cg),
        eyepoint = location(Some("EYEPOINT"), design.payload.station_m),
        vrp = location(Some("VRP"), DVec3::ZERO),
    )
}

fn mass_balance(design: &FixedWingDesign) -> String {
    let [column_x, column_y, column_z] = [
        design.empty_inertia_kg_m2.x_axis,
        design.empty_inertia_kg_m2.y_axis,
        design.empty_inertia_kg_m2.z_axis,
    ];
    let slug_ft2 = |kg_m2: f64| kg_m2 / KG_M2_PER_SLUG_FT2;
    // JSBSim negates its stored products of inertia when it builds the tensor,
    // and it builds it in body axes, which the package's root axes are.
    format!(
        "  <mass_balance>
    <ixx unit=\"SLUG*FT2\"> {ixx:.4} </ixx>
    <iyy unit=\"SLUG*FT2\"> {iyy:.4} </iyy>
    <izz unit=\"SLUG*FT2\"> {izz:.4} </izz>
    <ixy unit=\"SLUG*FT2\"> {ixy:.4} </ixy>
    <ixz unit=\"SLUG*FT2\"> {ixz:.4} </ixz>
    <iyz unit=\"SLUG*FT2\"> {iyz:.4} </iyz>
    <emptywt unit=\"LBS\"> {empty:.4} </emptywt>
    {cg}
    <pointmass name=\"payload\">
      <weight unit=\"LBS\"> {payload:.4} </weight>
      {payload_at}
    </pointmass>
  </mass_balance>",
        ixx = slug_ft2(column_x.x),
        iyy = slug_ft2(column_y.y),
        izz = slug_ft2(column_z.z),
        ixy = slug_ft2(-column_y.x),
        ixz = slug_ft2(-column_z.x),
        iyz = slug_ft2(-column_z.y),
        empty = design.empty.mass_kg * POUNDS_PER_KG,
        cg = location(Some("CG"), design.empty.station_m),
        payload = design.payload.mass_kg * POUNDS_PER_KG,
        payload_at = location(None, design.payload.station_m),
    )
}
