use glam::DVec3;

use super::location;
use crate::FixedWingDesign;

/// How far the main wheels sit below the empty centre of gravity, metres: it
/// keeps a 1.9 m propeller clear of the ground with the struts compressed.
pub(crate) const MAIN_GEAR_DROP_M: f64 = 1.3;
/// How far the main wheels sit aft of the empty centre of gravity, metres,
/// and so behind the loaded one, or the aircraft would sit on its tail.
const MAIN_GEAR_AFT_M: f64 = 0.4;
/// Half the main wheel track, metres.
const MAIN_GEAR_HALF_TRACK_M: f64 = 1.27;
/// Nose wheel ahead of the empty centre of gravity, metres.
const NOSE_GEAR_AHEAD_M: f64 = 1.8;

/// A wheel; `brake` is its JSBSim brake group.
struct Bogey<'a> {
    name: &'a str,
    station_m: DVec3,
    spring_lb_ft: u32,
    damping_lb_ft_s: u32,
    rebound_lb_ft_s: u32,
    brake: &'a str,
}

impl Bogey<'_> {
    fn xml(&self) -> String {
        format!(
            "    <contact type=\"BOGEY\" name=\"{name}\">
      {at}
      <static_friction> 0.8 </static_friction>
      <dynamic_friction> 0.5 </dynamic_friction>
      <rolling_friction> 0.02 </rolling_friction>
      <spring_coeff unit=\"LBS/FT\"> {spring} </spring_coeff>
      <damping_coeff unit=\"LBS/FT/SEC\"> {damping} </damping_coeff>
      <damping_coeff_rebound unit=\"LBS/FT/SEC\"> {rebound} </damping_coeff_rebound>
      <max_steer unit=\"DEG\"> 0 </max_steer>
      <brake_group> {brake} </brake_group>
      <retractable> 0 </retractable>
    </contact>
",
            name = self.name,
            at = location(None, self.station_m),
            spring = self.spring_lb_ft,
            damping = self.damping_lb_ft_s,
            rebound = self.rebound_lb_ft_s,
            brake = self.brake,
        )
    }
}

/// Fixed tricycle gear of light-aeroplane stiffness with brakes on the mains,
/// plus a belly contact that stops a gear collapse. A conceptual design states
/// no gear, so all of it hangs off the empty centre of gravity.
pub(super) fn ground_reactions(design: &FixedWingDesign) -> String {
    let datum = design.empty.station_m;
    let main = |side: f64| {
        datum
            + DVec3::new(
                MAIN_GEAR_AFT_M,
                side * MAIN_GEAR_HALF_TRACK_M,
                -MAIN_GEAR_DROP_M,
            )
    };
    let wheels = [
        Bogey {
            name: "NOSE",
            station_m: datum + DVec3::new(-NOSE_GEAR_AHEAD_M, 0.0, -MAIN_GEAR_DROP_M),
            spring_lb_ft: 1_800,
            damping_lb_ft_s: 500,
            rebound_lb_ft_s: 2_000,
            brake: "NONE",
        },
        Bogey {
            name: "LEFT_MAIN",
            station_m: main(-1.0),
            spring_lb_ft: 5_400,
            damping_lb_ft_s: 600,
            rebound_lb_ft_s: 1_200,
            brake: "LEFT",
        },
        Bogey {
            name: "RIGHT_MAIN",
            station_m: main(1.0),
            spring_lb_ft: 5_400,
            damping_lb_ft_s: 600,
            rebound_lb_ft_s: 1_200,
            brake: "RIGHT",
        },
    ];
    let wheels: String = wheels.iter().map(Bogey::xml).collect();
    format!(
        "  <ground_reactions>
{wheels}    <contact type=\"STRUCTURE\" name=\"BELLY\">
      {belly}
      <static_friction> 0.8 </static_friction>
      <dynamic_friction> 0.5 </dynamic_friction>
      <spring_coeff unit=\"LBS/FT\"> 20000 </spring_coeff>
      <damping_coeff unit=\"LBS/FT/SEC\"> 3000 </damping_coeff>
    </contact>
  </ground_reactions>",
        belly = location(None, datum - DVec3::Z),
    )
}
