"""Generate constant-property forebody reference cases without external gas tables."""
import math
from pathlib import Path


def prepare(directory, case, steps, vertices=None, *, outlet="native"):
    try:
        outlet_bc = {
            "native": "OutFlowBC_Simple:new{}",
            "zero-normal-gradient": "OutFlowBC_SimpleFlux:new{zeroNormalViscousGradients=true}",
        }[outlet]
    except KeyError as error:
        raise ValueError(f"Unsupported Eilmer outlet selection: {outlet}") from error
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=False)
    g, gas, inflow = case["grid"], case["gas"], case["inflow"]
    nr, nt = g["radial_cells"], g["angular_cells"]
    if vertices is None:
        vertices = case.get("mesh_vertices")
    if vertices is None and g.get("shock_shape_mach") is not None:
        raise ValueError("Shaped grid comparison requires exported Rust mesh vertices")
    if vertices is not None and len(vertices) != (nr + 1) * (nt + 1):
        raise ValueError("Exported mesh vertex count does not match case resolution")
    points = []
    for j in range(nt + 1):
        theta = math.pi * j / (2 * nt)
        for i in range(nr + 1):
            if vertices is not None:
                point = vertices[j * (nr + 1) + nr - i]
                points.append(" ".join(f"{v:.17g}" for v in point))
                continue
            eta = 1 - i / nr
            f = math.expm1(g["stretching"] * eta) / math.expm1(g["stretching"]) if g["stretching"] else eta
            r = g["radius"] + (g["outer_radius"] - g["radius"]) * f
            points.append(f"{-r * math.cos(theta):.17g} {r * math.sin(theta):.17g} 0")
    (directory / "mesh.vtk").write_text(
        f"# vtk DataFile Version 2.0\nACE sphere\nASCII\nDATASET STRUCTURED_GRID\n"
        f"DIMENSIONS {nr+1} {nt+1} 1\nPOINTS {len(points)} double\n" + "\n".join(points) + "\n")
    cp = gas["gamma"] * gas["gas_constant"] / (gas["gamma"] - 1)
    (directory / "gas.lua").write_text(f"""model = 'IdealGas'
IdealGas = {{
 speciesName='benchmark-gas', mMass={8.31451/gas['gas_constant']:.17g}, gamma={gas['gamma']:.17g},
 entropyRefValues={{s1=0.0,T1=300.0,p1=100000.0}},
 viscosity={{model='CEA',nsegments=1,
 segment0={{T_lower=1.0,T_upper=100000.0,A=0.0,B=0.0,C=0.0,D={math.log(gas['viscosity']/1e-7):.17g}}}}},
 thermCondModel={{model='constPrandtl',Prandtl={gas['viscosity']*cp/gas['conductivity']:.17g}}}
}}
""")
    temperature = inflow["pressure"] / (inflow["density"] * gas["gas_constant"])
    (directory / "sphere.lua").write_text(f"""config.title = 'ACE ideal-gas forebody reference'
setGasModel('gas.lua')
inflow = FlowState:new{{p={inflow['pressure']:.17g}, T={temperature:.17g}, velx={inflow['velocity'][0]:.17g}}}
config.axisymmetric = true
config.viscous = true
config.flow_format = 'rawbinary'
config.flux_calculator = 'adaptive_hlle_hllc'
config.interpolation_order = 2
config.suppress_reconstruction_at_shocks = true
config.gasdynamic_update_scheme = 'predictor-corrector'
config.with_local_time_stepping = true
config.spatial_deriv_calc = 'least_squares'
config.spatial_deriv_locn = 'cells'
config.cfl_value = 0.5
config.viscous_signal_factor = 1.0
config.max_time = 1.0
config.max_step = {steps}
config.dt_init = 1.0e-10
config.dt_plot = 1.0
config.dt_loads = 2.0e-6
config.write_loads = true
config.save_residual_values = true
grid = StructuredGrid:new{{filename='mesh.vtk',fmt='vtk'}}
block = FluidBlock:new{{grid=grid,initialState=inflow,
 bcList={{west=InFlowBC_Supersonic:new{{flowState=inflow}},
 east=WallBC_NoSlip_FixedT:new{{Twall={case['wall_temperature']:.17g},group='loads'}},
 north={outlet_bc}}}}}
""")
