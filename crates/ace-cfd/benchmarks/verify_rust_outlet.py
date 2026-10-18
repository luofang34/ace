# SPDX-License-Identifier: AGPL-3.0-only
"""Verify a curved Rust outlet on a saved state without advancing the solution."""
import argparse
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
import shutil
import struct
import subprocess


def gauss_points(n=12):
    """Integrate cell moments independently of the solver's quadrature tables."""
    def legendre(z):
        previous, value = 1.0, z
        for degree in range(2, n + 1):
            previous, value = value, ((2 * degree - 1) * z * value
                                      - (degree - 1) * previous) / degree
        return value, n * (z * value - previous) / (z * z - 1)

    points = []
    for k in range(1, n + 1):
        z = math.cos(math.pi * (k - 0.25) / (n + 0.5))
        for _ in range(32):
            value, derivative = legendre(z)
            change = value / derivative
            z -= change
            if abs(change) < 2e-16:
                break
        else:
            raise RuntimeError("Independent Gauss quadrature did not converge")
        _, derivative = legendre(z)
        points.append(((z + 1) / 2, 1 / ((1 - z * z) * derivative**2)))
    if any(abs(math.fsum(w * x**k for x, w in points) - 1 / (k + 1)) > 1e-14
           for k in range(2 * n)):
        raise RuntimeError("Independent Gauss quadrature failed polynomial moments")
    return points


def cell_moments(vertices, nr, nt):
    """Integrate even/odd velocity-reconstruction moments in the outlet row."""
    points = gauss_points()
    j = nt - 1
    angles = [math.atan2(vertices[k * (nr + 1)][1], -vertices[k * (nr + 1)][0])
              for k in (j, j + 1)]
    moments = {}
    for i in range(nr):
        radii = [math.hypot(*vertices[l * (nr + 1) + k])
                 for k, l in ((i, j), (i, j + 1), (i + 1, j), (i + 1, j + 1))]
        samples = []
        for t, wt in points:
            theta = angles[0] + t * (angles[1] - angles[0])
            lo, hi = ((1 - t) * radii[k] + t * radii[k + 1] for k in (0, 2))
            for s, ws in points:
                r = lo + s * (hi - lo)
                x, y = -r * math.cos(theta), r * math.sin(theta)
                volume = wt * ws * (angles[1] - angles[0]) * (hi - lo) * r * y
                samples.append((volume, x, y))
        volume = math.fsum(dv for dv, x, y in samples)
        y_weight = math.fsum(dv * y for dv, x, y in samples)
        moments[j * nr + i] = (
            math.fsum(dv * y*y for dv, x, y in samples) / volume,
            math.fsum(dv * x*y for dv, x, y in samples) / y_weight,
            math.fsum(dv * y**3 for dv, x, y in samples) / y_weight)
    return moments


def viscous_flux(velocity, gradient, center, normal, gas, project=True):
    """Newtonian traction and Fourier flux, including cylindrical divergence."""
    nx, ny, _ = normal
    derivatives = []
    for index in (1, 2, 5):
        gx, gy, _ = gradient[index]
        normal_derivative = gx * nx + gy * ny if project else 0.0
        derivatives.append((gx - normal_derivative * nx, gy - normal_derivative * ny))
    (ux, uy), (vx, vy), (tx, ty) = derivatives
    u, v = velocity
    divergence = ux + vy + v / center[1]
    mu = gas["viscosity"]
    xx, yy = mu * (2 * ux - 2 * divergence / 3), mu * (2 * vy - 2 * divergence / 3)
    xy = mu * (uy + vx)
    traction = xx * nx + xy * ny, xy * nx + yy * ny
    energy = u * traction[0] + v * traction[1] + gas["conductivity"] * (tx * nx + ty * ny)
    return [0.0, traction[0], traction[1], 0.0, energy]


def point_fluxes(cell, face, moments, gas):
    """Recover the viscous point velocity and equal-state Euler convection."""
    q, g = cell["primitive"], cell["gradient"]
    x, y, _ = face["center"]
    cx, cy, _ = cell["center"]
    mean_y2, odd_x, odd_y2 = moments
    u = q["velocity"][0] + g[1][0] * (x - cx) + g[1][1] * (y*y - mean_y2) / (2 * cy)
    v = y * (q["velocity"][1] / cy + g[2][0] / cy * (x - odd_x)
             + (g[2][1] - q["velocity"][1] / cy) / cy * (y*y - odd_y2) / (2 * cy))
    expected = viscous_flux((u, v), g, face["center"], face["normal"], gas)
    unprojected = viscous_flux((u, v), g, face["center"], face["normal"], gas, False)
    rho, p = q["density"], q["pressure"]
    qx, qy, qz = q["velocity"]
    nx, ny, _ = face["normal"]
    speed = qx * nx + qy * ny
    energy = p / (gas["gamma"] - 1) + 0.5 * rho * (qx*qx + qy*qy + qz*qz)
    convective = [rho * speed, rho * speed * qx + p * nx, rho * speed * qy + p * ny,
                  rho * speed * qz, (energy + p) * speed]
    measured = [a - b for a, b in zip(convective, face["total_flux"], strict=True)]
    return measured, expected, unprojected


def require_finite(value):
    """Reject non-finite evidence rather than letting NaN bypass comparisons."""
    if isinstance(value, dict):
        for item in value.values():
            require_finite(item)
    elif isinstance(value, list):
        for item in value:
            require_finite(item)
    elif isinstance(value, (float, int)) and not math.isfinite(value):
        raise ValueError("Non-finite input or diagnostic evidence")


def validate_case(case, saved):
    """Restrict the audit to its explicit curved, constant-gas, no-swirl scope."""
    require_finite(case)
    require_finite(saved)
    if case.get("outflow") != "zero_gradient_cell_flux" or not case.get("curved_geometry"):
        raise ValueError("Outlet audit requires curved zero_gradient_cell_flux")
    nr, nt = (case["grid"][key] for key in ("radial_cells", "angular_cells"))
    if any(type(n) is not int or n < 2 for n in (nr, nt)):
        raise ValueError("Outlet audit requires at least two cells per direction")
    vertices = case.get("mesh_vertices", [])
    if (len(vertices) != (nr + 1) * (nt + 1) or vertices != saved.get("mesh_vertices")
            or not saved.get("curved_geometry")):
        raise ValueError("Explicit case and saved-state meshes must match")
    if any(len(p) != 3 or p[2] != 0 for p in vertices):
        raise ValueError("Outlet audit requires planar meridional vertices")
    angles = []
    for j in range(nt + 1):
        row = vertices[j * (nr + 1):(j + 1) * (nr + 1)]
        theta = math.atan2(row[0][1], -row[0][0])
        if any(abs(math.atan2(p[1], -p[0]) - theta) > 1e-12 for p in row):
            raise ValueError("Curved cell integration requires radial mesh lines")
        if any(math.hypot(*a) >= math.hypot(*b) for a, b in zip(row, row[1:])):
            raise ValueError("Curved radial coordinates must increase")
        angles.append(theta)
    if (abs(angles[0]) > 1e-12 or abs(angles[-1] - math.pi / 2) > 1e-12
            or any(a >= b for a, b in zip(angles, angles[1:]))):
        raise ValueError("Outlet audit requires an axis-to-equator forebody")
    state = saved["solution"]["state"]
    if len(state) != nr * nt or any(len(q) != 5 or q[3] != 0 for q in state):
        raise ValueError("Saved state must match the grid and have zero swirl")
    gas = case["gas"]
    if (gas["gamma"] <= 1 or any(gas[k] <= 0 for k in
            ("gas_constant", "viscosity", "conductivity"))):
        raise ValueError("Positive constant ideal-gas transport properties are required")


def compare_traces(case, saved, diagnostic, trace):
    """Check every native outlet point against independently recovered fluxes."""
    require_finite(diagnostic)
    require_finite(trace)
    nr, nt = (case["grid"][key] for key in ("radial_cells", "angular_cells"))
    cells = diagnostic["cells"]
    if len(cells) != nr * nt or not diagnostic["curved_geometry"]:
        raise RuntimeError("Diagnostic cells do not match the curved saved mesh")
    for i, (cell, state) in enumerate(zip(cells, saved["solution"]["state"], strict=True)):
        if cell["cell"] != i or struct.pack("5d", *cell["mean_state"]) != struct.pack("5d", *state):
            raise RuntimeError("Diagnostic evaluation changed conserved state bits")
        if cell["primitive"]["velocity"][2] != 0 or any(cell["gradient"][3]):
            raise RuntimeError("Diagnostic evidence contains unsupported swirl")
    if diagnostic["balance"] != trace["balance"]:
        raise RuntimeError("Cell and face evaluations disagree on the energy balance")
    moments = cell_moments(case["mesh_vertices"], nr, nt)
    errors, differences, powers, points, owners, face_owners = [], [], [], set(), Counter(), {}
    for face in trace["faces"]:
        if face["neighbor"] is not None or abs(face["center"][0]) > 1e-12 * case["grid"]["radius"]:
            continue
        owner = face["owner"]
        if owner not in moments:
            raise RuntimeError("Outlet face owner is outside the last angular row")
        cell = cells[owner]
        if face["left"] != cell["primitive"] or face["right"] != cell["primitive"]:
            raise RuntimeError("Outlet convection must use the adjacent cell state")
        nx, ny, nz = face["normal"]
        if (abs(nx - 1) > 1e-14 or abs(ny) > 1e-14 or nz != 0
                or face["center"][1] <= 0 or face["area"] <= 0):
            raise RuntimeError("Invalid equatorial outlet point geometry")
        key = (face["face"], *face["center"])
        if key in points or face_owners.get(face["face"], owner) != owner:
            raise RuntimeError("Duplicate or inconsistent outlet quadrature point")
        points.add(key)
        owners[owner] += 1
        face_owners[face["face"]] = owner
        measured, expected, unprojected = point_fluxes(cell, face, moments[owner], case["gas"])
        errors.extend(abs(a - b) / max(1, abs(b)) for a, b in zip(measured, expected, strict=True))
        differences.extend(abs(a - b) / max(1, abs(b)) for a, b in zip(measured, unprojected, strict=True))
        powers.append(-expected[4] * face["area"] * math.tau)
    if owners != Counter({owner: 4 for owner in moments}) or len(face_owners) != nr:
        raise RuntimeError("Outlet must contain four distinct points per radial face")
    error, difference = max(errors), max(differences)
    return dict(qualified=error < 1e-8 and difference > 1e-6,
                quadrature_points=len(points), maximum_scaled_viscous_flux_error=error,
                unprojected_gradient_difference=difference,
                viscous_outward_power=math.fsum(powers), conserved_bits_identical=True,
                convective_states_equal_adjacent_cell=True, iterations_advanced=0,
                spatial_qualified=False, timing_qualified=False)


def digest_blocking(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def verify_blocking(case_path, saved_path, executable, output):
    """Freeze and bind inputs, then run the native diagnostic paths only."""
    original = {p: digest_blocking(p) for p in (case_path, saved_path, executable, Path(__file__).resolve())}
    case, saved = json.loads(case_path.read_text()), json.loads(saved_path.read_text())
    validate_case(case, saved)
    output.mkdir(parents=True, exist_ok=False)
    frozen = {}
    for source, name in ((case_path, "input.json"), (saved_path, "saved.json"), (executable, "sphere")):
        target = output / name
        shutil.copy2(source, target)
        if digest_blocking(target) != original[source]:
            raise RuntimeError("Input changed while freezing the outlet audit")
        frozen[target] = original[source]
    for mode in ("diagnose", "faces"):
        with (output / f"{mode}.log").open("w") as log:
            subprocess.run([str(output / "sphere"), str(output / "input.json"),
                            str(output / f"{mode}.json"), str(output / "saved.json"), f"--{mode}"],
                           stdout=log, stderr=subprocess.STDOUT, check=True)
    result = compare_traces(case, saved, json.loads((output / "diagnose.json").read_text()),
                            json.loads((output / "faces.json").read_text()))
    for path, expected in (original | frozen).items():
        if digest_blocking(path) != expected:
            raise RuntimeError(f"Outlet audit input changed: {path}")
    evidence = list(original | frozen) + [output / f"{name}.json" for name in ("diagnose", "faces")]
    result.update(inputs={str(p): digest_blocking(p) for p in evidence}, saved_state_unchanged=True,
                  scope="Constant-property, no-swirl curved forebody outlet operation only; "
                        "does not establish interior gradient accuracy or spatial convergence")
    (output / "result.json").write_text(json.dumps(result, indent=2))
    if not result["qualified"]:
        raise RuntimeError("Outlet flux check failed or cannot distinguish gradient projection")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("case", "saved", "executable", "output"):
        parser.add_argument(f"--{name}", required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(verify_blocking(args.case.resolve(), args.saved.resolve(),
                                    args.executable.resolve(), args.output.resolve())))


if __name__ == "__main__":
    main()
