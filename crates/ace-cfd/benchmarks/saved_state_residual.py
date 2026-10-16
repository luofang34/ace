"""Evaluate the saved Eilmer state independently of its iteration-log residual."""
import hashlib
import json
import math
from pathlib import Path

from trace_eilmer import read_trace_blocking, trace_blocking

RELATIVE_RESIDUAL_TARGET = 1.0e-12


def verify_saved_state_blocking(directory, executable, case, env):
    """Use a copied checkpoint; verification time is excluded from solver timing."""
    output = directory.with_name(directory.name + "-saved-state-check")
    trace = trace_blocking(directory, output, executable, Path(env["DGD"]), case,
                           state_purity=True)
    cells = read_trace_blocking(output / "trace-cells.csv")
    norm = math.sqrt(math.fsum(cell[key]**2 for cell in cells
                             for key in ("mass", "momx", "momy", "energy")))
    reference_bytes = (directory / "sphere-ref-residuals.saved").read_bytes()
    reference = float(reference_bytes.split()[0])
    relative = norm / reference
    qualified = (math.isfinite(relative) and relative <= RELATIVE_RESIDUAL_TARGET
                 and trace["state_purity"]["passed"] and trace["checkpoint_unchanged"]
                 and trace["normalized_residual"] < case["solver"]["residual_tolerance"])
    result = dict(qualified=qualified, global_residual=norm, relative_global_residual=relative,
                  reference_residual=reference,
                  reference_sha256=hashlib.sha256(reference_bytes).hexdigest(),
                  executable_sha256=trace["executable_sha256"],
                  normalized_residual=trace["normalized_residual"],
                  checkpoint_hashes=trace["checkpoint_hashes"], iterations_advanced=0,
                  state_purity=trace["state_purity"], verification_directory=str(output))
    (output / "saved-state-residual.json").write_text(json.dumps(result, indent=2))
    return result
