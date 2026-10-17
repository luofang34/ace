# SPDX-License-Identifier: AGPL-3.0-only
"""Keep benchmark execution independent of a shell's active Conda environment."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class PythonToolchainTests(unittest.TestCase):
    def test_uv_uses_the_pinned_managed_python_with_conda_first_on_path(self):
        root = Path(__file__).resolve().parents[3]
        uv = shutil.which("uv")
        self.assertIsNotNone(uv, "Install uv to run the benchmark checks")
        expected = (root / ".python-version").read_text().strip()
        managed = Path(subprocess.check_output([uv, "python", "dir"], text=True).strip()).resolve()
        with tempfile.TemporaryDirectory() as temporary:
            conda = Path(temporary) / "anaconda3"
            binaries = conda / "bin"
            binaries.mkdir(parents=True)
            for name in ("python", "python3", "python" + expected.rsplit(".", 1)[0]):
                path = binaries / name
                path.write_text("#!/bin/sh\nexit 73\n")
                path.chmod(0o755)
            env = dict(os.environ, PATH=str(binaries) + os.pathsep + os.environ["PATH"],
                       CONDA_PREFIX=str(conda), CONDA_DEFAULT_ENV="base", CONDA_SHLVL="1",
                       UV_CACHE_DIR=str(Path(temporary) / "cache"))
            code = ("import json, platform, sys; "
                    "print(json.dumps([platform.python_version(), sys.executable, sys.base_prefix]))")
            result = subprocess.run([uv, "run", "--no-project", "--offline", "--no-python-downloads",
                                     "python", "-c", code], cwd=root, env=env,
                                    text=True, capture_output=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stderr)
            version, executable, prefix = json.loads(result.stdout)
            self.assertEqual(version, expected)
            self.assertTrue(Path(executable).resolve().is_relative_to(managed))
            self.assertTrue(Path(prefix).resolve().is_relative_to(managed))


if __name__ == "__main__":
    unittest.main()
