use std::fmt::Write;

use crate::FixedWingDesign;

/// Representative stability and control derivatives of a stable light
/// aeroplane, per radian or per non-dimensional rate. `ace-rs` models
/// performance, not handling, so none of these comes from the design.
const CM_ALPHA: f64 = -0.9;
const CM_Q: f64 = -14.0;
const CM_ELEVATOR: f64 = -1.2;
const CY_BETA: f64 = -0.5;
const CL_BETA: f64 = -0.08;
const CL_P: f64 = -0.45;
const CL_R: f64 = 0.10;
const CL_AILERON: f64 = 0.20;
const CN_BETA: f64 = 0.12;
const CN_R: f64 = -0.15;
const CN_RUDDER: f64 = -0.12;
/// Nose-down pitch of full flap.
const CM_FLAP: f64 = -0.08;

/// Past the stall the wing keeps this share of `CLmax` at these angles beyond
/// it, radians: a gentle break, then the plateau of a stalled wing.
const STALL_BREAK: [(f64, f64); 2] = [(0.12, 0.76), (0.50, 0.55)];

/// The stick and the trim wheel, summed before the surface moves.
const PITCH_SUM: &str = "      <summer name=\"fcs/pitch-sum\">
        <input>fcs/elevator-cmd-norm</input>
        <input>fcs/pitch-trim-cmd-norm</input>
        <clipto> <min> -1 </min> <max> 1 </max> </clipto>
      </summer>
";

/// Six seconds from up to full.
const FLAPS: &str = "    <channel name=\"Flaps\">
      <kinematic name=\"fcs/flap-control\">
        <input>fcs/flap-cmd-norm</input>
        <traverse>
          <setting> <position> 0 </position> <time> 0 </time> </setting>
          <setting> <position> 1 </position> <time> 6 </time> </setting>
        </traverse>
        <output>fcs/flap-pos-norm</output>
      </kinematic>
    </channel>
";

/// The surfaces' full deflection and the command path every aircraft has, plus
/// what `links` adds. `pure_gain`s and the like go in there.
pub(super) fn flight_control(links: &str) -> String {
    let surface = |channel: &str, sum: &str, control: &str, command: &str, position: &str| {
        format!(
            "    <channel name=\"{channel}\">
{sum}      <aerosurface_scale name=\"fcs/{control}\">
        <input>fcs/{command}</input>
        <range> <min> -0.35 </min> <max> 0.35 </max> </range>
        <output>fcs/{position}</output>
      </aerosurface_scale>
    </channel>
"
        )
    };
    format!(
        "  <flight_control name=\"fcs\">\n{}{}{}{FLAPS}{links}  </flight_control>",
        surface(
            "Pitch",
            PITCH_SUM,
            "elevator-control",
            "pitch-sum",
            "elevator-pos-rad"
        ),
        surface(
            "Roll",
            "",
            "aileron-control",
            "aileron-cmd-norm",
            "left-aileron-pos-rad"
        ),
        surface(
            "Yaw",
            "",
            "rudder-control",
            "rudder-cmd-norm",
            "rudder-pos-rad"
        ),
    )
}

/// `CL(α)`: the linear range up to `CLmax`, then the stall.
fn lift_table(design: &FixedWingDesign) -> String {
    let slope = design.lift_slope_per_rad();
    let zero_lift = design.zero_lift_alpha_rad();
    let stall = design.cl_max / slope + zero_lift;
    let mut rows = vec![
        (zero_lift - design.cl_max / slope, -design.cl_max),
        (zero_lift, 0.0),
        (stall, design.cl_max),
    ];
    rows.extend(
        STALL_BREAK
            .iter()
            .map(|(beyond, share)| (stall + beyond, design.cl_max * share)),
    );
    let mut table = String::new();
    for (alpha, lift) in rows {
        // Writing to a `String` cannot fail.
        writeln!(table, "              {alpha:.5} {lift:.5}").ok();
    }
    table
}

fn axis(name: &str, description: &str, reference: &str, body: &str) -> String {
    format!(
        "    <axis name=\"{name}\">
      <function name=\"aero/{description}\">
        <description>{description}</description>
        <product>
          <property>aero/qbar-psf</property>
          <property>metrics/Sw-sqft</property>
{reference}{body}        </product>
      </function>
    </axis>
"
    )
}

fn term(value: f64, properties: &[&str]) -> String {
    let properties: String = properties
        .iter()
        .map(|property| format!(" <property>{property}</property>"))
        .collect();
    format!("            <product> <value> {value:.6} </value>{properties} </product>\n")
}

fn sum(terms: &[String]) -> String {
    format!("          <sum>\n{}          </sum>\n", terms.concat())
}

/// The six axes. `Cm0` trims the loaded aircraft at `cruise_alpha_rad`.
pub(super) fn aerodynamics(design: &FixedWingDesign, cruise_alpha_rad: f64) -> String {
    const SPAN: &str = "          <property>metrics/bw-ft</property>\n";
    const CHORD: &str = "          <property>metrics/cbarw-ft</property>\n";
    let clean_lift = format!(
        "            <table>
              <independentVar lookup=\"row\">aero/alpha-rad</independentVar>
              <tableData>
{}              </tableData>
            </table>\n",
        lift_table(design),
    );
    let lift = sum(&[
        clean_lift,
        term(design.flaps.delta_cl, &["fcs/flap-pos-norm"]),
    ]);
    let drag = sum(&[
        format!("            <value> {:.6} </value>\n", design.cd0),
        term(design.induced_drag_factor(), &["aero/cl-squared"]),
        term(design.flaps.delta_cd, &["fcs/flap-pos-norm"]),
    ]);
    let side = format!(
        "          <property>aero/beta-rad</property>\n          <value> {CY_BETA:.6} </value>\n"
    );
    let roll = sum(&[
        term(CL_BETA, &["aero/beta-rad"]),
        term(CL_P, &["aero/bi2vel", "velocities/p-aero-rad_sec"]),
        term(CL_R, &["aero/bi2vel", "velocities/r-aero-rad_sec"]),
        term(CL_AILERON, &["fcs/left-aileron-pos-rad"]),
    ]);
    let pitch = sum(&[
        format!(
            "            <value> {:.6} </value>\n",
            -CM_ALPHA * cruise_alpha_rad
        ),
        term(CM_ALPHA, &["aero/alpha-rad"]),
        term(CM_Q, &["aero/ci2vel", "velocities/q-aero-rad_sec"]),
        term(CM_ELEVATOR, &["fcs/elevator-pos-rad"]),
        term(CM_FLAP, &["fcs/flap-pos-norm"]),
    ]);
    let yaw = sum(&[
        term(CN_BETA, &["aero/beta-rad"]),
        term(CN_R, &["aero/bi2vel", "velocities/r-aero-rad_sec"]),
        term(CN_RUDDER, &["fcs/rudder-pos-rad"]),
    ]);
    format!(
        "  <aerodynamics>\n{}{}{}{}{}{}  </aerodynamics>",
        axis("LIFT", "force/lift", "", &lift),
        axis("DRAG", "force/drag", "", &drag),
        axis("SIDE", "force/side", "", &side),
        axis("ROLL", "moment/roll", SPAN, &roll),
        axis("PITCH", "moment/pitch", CHORD, &pitch),
        axis("YAW", "moment/yaw", SPAN, &yaw),
    )
}
