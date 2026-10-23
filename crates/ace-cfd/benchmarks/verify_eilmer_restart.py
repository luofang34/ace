"""Exercise native snapshot rotation and two restarts without changing stopping thresholds."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

from eilmer_restart import prepare_restart_blocking
from eilmer_snapshot import validate_snapshot_blocking


def verify_blocking(source, executable, installation, output):
    """Force snapshot-slot reuse in copied cases and require exact reference preservation."""
    output.mkdir()
    original_reference = (source / "sphere-ref-residuals.saved").read_bytes()
    original_controls = json.loads((source / "config/sphere.control").read_text())[
        "steady_state_solver_options"]
    threshold_keys = ("stop_on_relative_global_residual", "stop_on_absolute_global_residual",
                      "stop_on_mass_balance")
    env = dict(os.environ, DGD=str(installation),
               DGD_LUA_PATH=str(installation / "lib/?.lua"),
               DGD_LUA_CPATH=str(installation / "lib/?.so"))
    records = []
    for index in range(2):
        directory = output / f"restart-{index}"
        manifest = prepare_restart_blocking(source, directory)
        rows = [line.split() for line in (directory / "config/sphere.times").read_text().splitlines()
                if line.strip() and not line.startswith("#")]
        control_path = directory / "config/sphere.control"
        control = json.loads(control_path.read_text())
        options = control["steady_state_solver_options"]
        options["number_total_steps"] = manifest["step"] + 1
        options["number_total_snapshots"] = len(rows) - 1
        control_path.write_text(json.dumps(control, indent=2))
        if any(options[key] != original_controls[key] for key in threshold_keys):
            raise RuntimeError("Restart verification changed a stopping threshold")
        with (directory / "run.log").open("w") as log:
            subprocess.run([str(executable), *manifest["command_arguments"]], cwd=directory,
                           env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=240)
        if (directory / "sphere-ref-residuals.saved").read_bytes() != original_reference:
            raise RuntimeError("Native restart changed the saved residual reference")
        rows = [line.split() for line in (directory / "config/sphere.times").read_text().splitlines()
                if line.strip() and not line.startswith("#")]
        latest = max(rows, key=lambda row: int(row[4]))
        if int(latest[4]) != manifest["step"] + 1:
            raise RuntimeError("Native restart did not save the one accepted update")
        payload = validate_snapshot_blocking(directory, int(latest[0]), float(latest[1]))
        records.append(dict(step=int(latest[4]), snapshot_payload=payload,
                            reference_sha256=hashlib.sha256(original_reference).hexdigest()))
        source = directory
    result = dict(passed=True, records=records,
                  executable_sha256=hashlib.sha256(executable.read_bytes()).hexdigest(),
                  thresholds_unchanged=True, spatial_accuracy_qualified=False,
                  timing_qualified=False)
    (output / "restart-verification.json").write_text(json.dumps(result, indent=2))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("source", "executable", "installation", "output"):
        parser.add_argument("--" + name, required=True, type=Path)
    args = parser.parse_args()
    result = verify_blocking(*(getattr(args, name).resolve()
                              for name in ("source", "executable", "installation", "output")))
    print(json.dumps(result))


if __name__ == "__main__":
    main()
