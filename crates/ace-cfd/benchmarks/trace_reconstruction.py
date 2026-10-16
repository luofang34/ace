"""Check native Eilmer reconstruction of constant and affine velocity fields."""
import csv
import math


def check_reconstruction_blocking(directory):
    """Require all six stencils, both coordinate frames and clipping modes."""
    with (directory / "trace-constant-reconstruction.csv").open() as stream:
        rows = [{key: float(value) for key, value in row.items()}
                for row in csv.DictReader(stream)]
    if not rows or any(not math.isfinite(value) for row in rows for value in row.values()):
        raise ValueError("Empty or nonfinite reconstruction trace")
    expected_groups = {(kind, field, clip, local) for kind in range(6)
                       for field in range(3) for clip in range(2) for local in range(2)
                       if field != 2 or clip == 0}
    groups = {}
    changed, constant_error, affine_ulps = 0, 0.0, 0.0
    for row in rows:
        group = tuple(row[key] for key in ("kind", "field", "clip", "local_frame"))
        faces = groups.setdefault(group, set())
        if row["face"] in faces:
            raise ValueError("Duplicate face in reconstruction trace group")
        faces.add(row["face"])
        sides = ("left", "right")
        if row["field"] == 2 and row["kind"] in (4, 5):
            sides = ("left",) if row["kind"] == 4 else ("right",)
        differences = [(abs(row[f"{side}_{component}"] - row[f"expected_{component}"]),
                        math.ulp(row[f"expected_{component}"]))
                       for side in sides for component in ("u", "v")]
        if row["field"] == 2:
            affine_ulps = max(affine_ulps, *(error / ulp for error, ulp in differences))
        else:
            maximum = max(error for error, _ in differences)
            changed += any(row[f"{side}_{component}"].hex()
                           != row[f"expected_{component}"].hex()
                           for side in sides for component in ("u", "v"))
            constant_error = max(constant_error, maximum)
    complete = (set(groups) == expected_groups
                and all(faces == next(iter(groups.values())) for faces in groups.values()))
    return dict(passed=complete and changed == 0 and affine_ulps <= 64,
                complete_stencil_coverage=complete, rows=len(rows),
                changed_constant_rows=changed, maximum_constant_velocity_error=constant_error,
                maximum_affine_error_ulps=affine_ulps,
                scope="constant preservation and affine reconstruction; no time advancement")
