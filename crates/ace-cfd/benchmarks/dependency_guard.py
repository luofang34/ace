"""Reject unreviewed licenses in the Rust core, examples, tests, and build dependencies."""
import subprocess


def main():
    text = subprocess.check_output([
        "cargo", "tree", "--locked", "-p", "ace-cfd", "--target", "all",
        "--edges", "normal,build,dev", "--prefix", "none", "--format", "{p}|{l}"
    ], text=True)
    reviewed = {"AGPL-3.0-only", "MIT", "MIT OR Apache-2.0", "Apache-2.0 OR MIT", "Unlicense OR MIT",
                "(MIT OR Apache-2.0) AND Unicode-3.0"}
    packages = {}
    for line in text.splitlines():
        package, license_expression = line.split("|", 1)
        license_expression = license_expression.removesuffix(" (*)")
        if package.startswith("ace-cfd ") and license_expression != "AGPL-3.0-only":
            raise RuntimeError("The CFD component must declare AGPL-3.0-only")
        if license_expression not in reviewed:
            raise RuntimeError(f"Unreviewed dependency license: {package}: {license_expression}")
        packages[package] = license_expression
    for package, license_expression in sorted(packages.items()):
        print(f"{package}: {license_expression}")


if __name__ == "__main__":
    main()
