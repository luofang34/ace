"""Verify a selectable zero-normal outlet against native Eilmer on one saved state."""
import argparse
import json
import math
from pathlib import Path

from eilmer_restart import prepare_restart_blocking
from trace_eilmer import read_trace_blocking, trace_blocking


OUTLET_TYPE = "outflow_simple_flux_zero_normal_gradient"


def projected_viscous_flux(cell, face, gas):
    """Independent Newtonian stress/Fourier flux with normal derivatives removed."""
    nx, ny = face["nx"], face["ny"]

    def project(x, y):
        normal = x * nx + y * ny
        return x - normal * nx, y - normal * ny

    ux, uy = project(cell["dudx"], cell["dudy"])
    vx, vy = project(cell["dvdx"], cell["dvdy"])
    tx, ty = project(cell["gTx"], cell["gTy"])
    mu, conductivity = gas["viscosity"], gas["conductivity"]
    divergence = ux + vy + face["v"] / face["y"]
    xx, yy = mu * (2 * ux - 2 * divergence / 3), mu * (2 * vy - 2 * divergence / 3)
    xy = mu * (uy + vx)
    traction = xx * nx + xy * ny, xy * nx + yy * ny
    energy = (traction[0] * face["u"] + traction[1] * face["v"]
              + conductivity * (tx * nx + ty * ny))
    return dict(mass=0.0, momx=traction[0], momy=traction[1], energy=energy)


def compare_traces(native, projected, case):
    """Require unchanged convection/interior fluxes and independently computed outlet fluxes."""
    cells = {int(row["id"]): row for row in read_trace_blocking(native / "trace-cells.csv")}
    for row in read_trace_blocking(projected / "trace-cells.csv"):
        if any(row[key] != cells[int(row["id"])][key]
               for key in row if key not in ("mass", "momx", "momy", "energy")):
            raise RuntimeError("Outlet face projection changed a cell state or gradient")
    a = read_trace_blocking(native / "trace-inviscid.csv")
    b = read_trace_blocking(projected / "trace-inviscid.csv")
    # The inviscid-stage trace also contains gradients retained from the preceding
    # residual evaluation; those are not inputs to the convective face flux.
    if any(any(x[key] != y[key] for key in x if key not in ("gTx", "gTy"))
           for x, y in zip(a, b, strict=True)):
        raise RuntimeError("Outlet viscous selection changed the convective trace")
    convective = {int(row["id"]): row for row in a}
    native_faces = read_trace_blocking(native / "trace-total.csv")
    projected_faces = read_trace_blocking(projected / "trace-total.csv")
    errors, normal_derivatives, native_power, projected_power = [], [], 0.0, 0.0
    native_errors = []
    count = 0
    for original, face in zip(native_faces, projected_faces, strict=True):
        if any(face[key] != original[key]
               for key in face if key not in ("gTx", "gTy", "mass", "momx", "momy", "energy")):
            raise RuntimeError("Outlet selection changed face geometry or state")
        # The forebody outlet is the equatorial plane, away from the symmetry axis.
        outlet = abs(face["x"]) < 1e-12 * case["grid"]["radius"] and face["y"] > 0
        if not outlet:
            if face != original:
                raise RuntimeError("Outlet viscous selection changed a non-outlet face")
            continue
        count += 1
        adjacent = [cells[int(face[k])] for k in ("left", "right") if int(face[k]) in cells]
        if len(adjacent) != 1:
            raise RuntimeError("An outlet face must have exactly one interior cell")
        expected = projected_viscous_flux(adjacent[0], original, case["gas"])
        inviscid = convective[int(face["id"])]
        for key, value in expected.items():
            actual = inviscid[key] - face[key]
            errors.append(abs(actual - value) / max(1.0, abs(value)))
            native_errors.append(abs(inviscid[key] - original[key] - value) / max(1.0, abs(value)))
        normal = face["gTx"] * face["nx"] + face["gTy"] * face["ny"]
        normal_derivatives.append(abs(normal) / max(1.0, math.hypot(original["gTx"], original["gTy"])))
        area = math.tau * face["area"]
        projected_power -= case["gas"]["conductivity"] * normal * area
        native_power -= case["gas"]["conductivity"] * area * (
            original["gTx"] * original["nx"] + original["gTy"] * original["ny"])
    if count != case["grid"]["radial_cells"]:
        raise RuntimeError("Outlet face count does not match the configured mesh")
    error, normal_error = max(errors), max(normal_derivatives)
    distinguishable = max(native_errors) > 1e-6
    return dict(qualified=error < 1e-8 and normal_error < 1e-12 and distinguishable,
                outlet_faces=count, maximum_scaled_viscous_flux_error=error,
                native_operator_distinguishable=distinguishable,
                maximum_relative_normal_temperature_derivative=normal_error,
                convective_trace_unchanged=True, non_outlet_faces_unchanged=True,
                native_conductive_outward_power=native_power,
                projected_conductive_outward_power=projected_power,
                iterations_advanced=0, timing_qualified=False)


def verify_blocking(source, output, executable, installation, case):
    """Retain both boundary problems and verify without updating either saved flow."""
    output.mkdir(parents=True, exist_ok=False)
    changed = output / "selected-boundary"
    prepare_restart_blocking(source, changed)
    path = changed / "config/sphere.config"
    config = json.loads(path.read_text())
    boundary = config["block_0"]["boundary_north"]
    if boundary["type"] != "outflow_simple_flux":
        raise ValueError("A native SimpleFlux forebody outlet is required")
    boundary["type"] = OUTLET_TYPE
    path.write_text(json.dumps(config))
    traces = []
    for name, initial in (("native", source), ("projected", changed)):
        traces.append(trace_blocking(initial, output / name, executable, installation, case))
    result = compare_traces(output / "native", output / "projected", case)
    result["traces"] = traces
    result["scope"] = "Laminar constant-property single-temperature forebody outlet condition only"
    (output / "result.json").write_text(json.dumps(result, indent=2))
    if not result["qualified"]:
        raise RuntimeError("Selectable outlet failed the fixed-state viscous-flux check")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("source", "output", "executable", "installation", "case"):
        parser.add_argument(f"--{name}", required=True, type=Path)
    args = parser.parse_args()
    result = verify_blocking(args.source.resolve(), args.output.resolve(),
                            args.executable.resolve(), args.installation.resolve(),
                            json.loads(args.case.read_text()))
    print(json.dumps(result))


if __name__ == "__main__":
    main()
