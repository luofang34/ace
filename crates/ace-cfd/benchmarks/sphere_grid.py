# SPDX-License-Identifier: AGPL-3.0-only
"""Prepare a spherical forebody grid with an explicit hyperbolic guide."""
import argparse
import copy
import json
import math
from pathlib import Path


def dimensions(nr, nt):
    if any(type(n) is not int or n < 4 for n in (nr, nt)):
        raise ValueError("Each grid direction requires at least four cells")


def nested_case(template, nr, nt):
    """Retain supplied vertices exactly when selecting a coarser tensor grid."""
    dimensions(nr, nt)
    case = copy.deepcopy(template)
    vertices = template.get("mesh_vertices")
    if vertices is not None:
        mr, mt = (template["grid"][key] for key in ("radial_cells", "angular_cells"))
        dimensions(mr, mt)
        if mr % nr or mt % nt:
            raise ValueError("Requested cell counts must divide the supplied master grid")
        if len(vertices) != (mr + 1) * (mt + 1):
            raise ValueError("Master vertex count does not match its grid dimensions")
        if any(len(v) != 3 or not all(math.isfinite(x) for x in v) for v in vertices):
            raise ValueError("Master vertices must be finite Cartesian triples")
        case["mesh_vertices"] = [copy.deepcopy(vertices[j * (mt // nt) * (mr + 1) + i * (mr // nr)])
                                 for j in range(nt + 1) for i in range(nr + 1)]
    case["grid"].update(radial_cells=nr, angular_cells=nt)
    return case


def aligned_case(template, nr, nt, nose, curvature, stretching, mach):
    """Place the guide at half radial index and blend its shape away from the wall."""
    dimensions(nr, nt)
    if template.get("mesh_vertices") is not None:
        raise ValueError("Use an unmeshed case to generate vertices; supplied geometry is authoritative")
    radius = template["grid"]["radius"]
    if not all(math.isfinite(x) for x in (nose, curvature, stretching, radius, mach)):
        raise ValueError("Finite geometry parameters required")
    if nose <= 1 or curvature <= 0 or radius <= 0 or not 0 <= stretching <= 8:
        raise ValueError("Require nose > 1, curvature and radius > 0, and stretching in [0, 8]")
    if mach <= 1:
        raise ValueError("The hyperbolic guide requires Mach > 1")
    delta = nose - 1
    gap = (delta * radius * math.expm1(stretching) / math.expm1(stretching / 2)
           if stretching else 2 * delta * radius)
    b = mach * mach - 1
    offset = curvature * b
    c = nose * (nose + 2 * offset)
    vertices = []
    for j in range(nt + 1):
        theta = math.pi * j / (2 * nt)
        cosine, sine = math.cos(theta), math.sin(theta)
        shock = c / ((nose + offset) * cosine
                     + math.sqrt(offset**2 * cosine**2 + b * c * sine**2))
        shape = (shock - 1) / delta
        previous = -math.inf
        for i in range(nr + 1):
            eta = i / nr
            t = min(2 * eta, 1)
            weight = t * t * (3 - 2 * t)
            distance = (gap * math.expm1(stretching * eta) / math.expm1(stretching)
                        if stretching else gap * eta)
            r = radius + distance * (1 + weight * (shape - 1))
            if not math.isfinite(r) or r <= previous:
                raise ValueError("Guide parameters produce crossing radial grid lines")
            vertices.append([-r * cosine, r * sine, 0.0])
            previous = r
    case = copy.deepcopy(template)
    case["grid"].pop("shock_shape_mach", None)
    case["grid"].update(radial_cells=nr, angular_cells=nt,
                        stretching=stretching, outer_radius=radius + gap)
    case["mesh_vertices"] = vertices
    return case


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--radial-cells", required=True, type=int)
    parser.add_argument("--angular-cells", required=True, type=int)
    parser.add_argument("--nose", required=True, type=float, help="Guide nose distance / body radius")
    parser.add_argument("--curvature", required=True, type=float, help="Guide nose curvature radius / body radius")
    parser.add_argument("--stretching", required=True, type=float)
    parser.add_argument("--mach", required=True, type=float, help="Geometric guide Mach number")
    args = parser.parse_args()
    case = aligned_case(json.loads(args.case.read_text()), args.radial_cells, args.angular_cells,
                        args.nose, args.curvature, args.stretching, args.mach)
    with args.output.open("x") as output:
        json.dump(case, output)


if __name__ == "__main__":
    main()
