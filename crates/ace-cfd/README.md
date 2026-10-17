# ace-cfd

Experimental axisymmetric, laminar, compressible Navier–Stokes kernels for a spherical
forebody with an isothermal no-slip wall. SI units are used throughout. The gas is
calorically perfect with constant viscosity and thermal conductivity. Chemistry,
vibrational nonequilibrium, radiation, turbulence and rarefied flow are outside this model.

The benchmark scripts use the workspace-pinned Python through uv. Install uv, then
run `uv python install` from the workspace root. The workspace requires a uv-managed
interpreter, independently of any active Conda environment. The scripts use only
the Python standard library. Run their checks with
`uv run --no-project python -m unittest discover -s crates/ace-cfd/benchmarks`.

From the workspace root:

```sh
cargo test -p ace-cfd --all-targets
cargo build -p ace-cfd --release --example sphere
target/release/examples/sphere crates/ace-cfd/cases/sphere-viscous.json /tmp/sphere-result.json
uv run --no-project python crates/ace-cfd/benchmarks/dependency_guard.py
uv run --no-project python crates/ace-cfd/benchmarks/verify_conduction.py \
  --rust target/release/examples/sphere --output /tmp/conduction-verification
```

The example writes mesh vertices, conservative states, wall pressure and heat flux, convergence
history and elapsed times as JSON. A positive wall heat flux enters the body.
Wall records include `theta_squared_mean`, the angular area moment of their heat-flux
quadrature. Stagnation extrapolation uses this moment for face averages and theta
squared for point samples. Curved results lacking the moment must recover wall
samples from their saved state with `--diagnose` before this extrapolation is used.
An unconverged solve writes its result and returns a failure status. An optional
third argument loads conservative states from a result for continuation; benchmark
runs always start from the free stream.
Appending `--audit` after a saved result path evaluates its discrete energy budget
without advancing the solution. The output includes outward boundary powers,
the volume-integrated energy residual and their conservation closure, all in watts.
Use `--diagnose` in the same position to include cell residuals, gradients, shock
sensors and local time-step estimates. `--checkpoint=PATH` instead continues the
solve and atomically saves its accepted state at each convergence check. These
checkpoints can be restarted or diagnosed, but do not certify convergence.
`--faces` exports the reconstructed states and total flux at every face integration point.

The optional `outflow` setting supports boundary-sensitivity experiments:
`zero_gradient` imposes zero normal viscous gradients and is the default;
`one_sided` fits outlet gradients using interior and wall data without that constraint;
`cell_flux` uses the same one-sided gradients but takes the convective flux from the
adjacent cell. `zero_gradient_cell_flux` instead changes only the outlet convective
state, retaining the gradient fit and viscous condition of `zero_gradient`.
These choices change the numerical boundary problem and must be
compared on fixed grids before combining their refinement results.

Both supplied cases have Mach 3, a 10 mm radius and a 300 K wall. With Reynolds
number based on diameter, `sphere-viscous.json` has Re ≈ 403 and `sphere.json`
has Re ≈ 4032. These constant-property verification gases are not high-enthalpy air
models. Iterative convergence alone does not establish spatial accuracy.
The conduction check solves a stationary spherical shell on three grids and checks
its heat transfer against the analytic solution, including refinement of the error.

Optional `grid.shock_shape_mach` uses a Billig hyperbola to guide the grid lines.
It changes geometry only; the shock is still captured by the flow equations.
`outer_radius` specifies the far-field distance on the stagnation axis. Without
the guide, grid lines are concentric spheres. The comparison driver passes exported
Rust vertices to Eilmer so both solvers use the same faceted geometry.
The example also accepts `mesh_vertices` in the input: Cartesian points ordered
angle-major, from wall to far field, with `(radial_cells + 1) * (angular_cells + 1)`
entries. These coordinates override the grid generator and must preserve the
spherical wall, symmetry axis, equatorial outlet and convex cells.
Set `curved_geometry: true` to integrate polar cells with a spherical wall and
curved radial faces. Every angular row of supplied vertices must lie on one ray;
radial levels interpolate linearly between angular nodes. Wall reconstruction
uses conservative cell averages and radial moments. Interior reconstruction uses
axisymmetric cell moments and removes the leading kinetic-energy variance when
recovering mean pressure; saved states must contain conservative cell averages.
Use
`verify_conduction.py --curved-geometry` for the analytic conduction check.
The Eilmer exporter uses faceted cells, so identical vertices do not make the two
geometric discretizations identical. Keep their refinement families separate.

For a serial comparison against a separately built Eilmer installation:

Build the reference executables with the AGPLv3 gzip adapter in this repository
and the residual, state-purity, snapshot and restart patches described below.
It uses the free zlib library through D's standard library bindings. From the
workspace root, with LDC on `PATH`:

```sh
uv run --no-project python crates/ace-cfd/benchmarks/verify_gzip.py --compiler /path/to/ldc2
make -C /path/to/gdtk/src/eilmer DMD=ldc2 FLAVOUR=fast \
  -W "$PWD/crates/ace-cfd/benchmarks/eilmer_gzip/gzip.d" \
  GZIP_DIR="$PWD/crates/ace-cfd/benchmarks/eilmer_gzip" e4shared e4-nk-shared
```

The forced prerequisite prevents reuse of an executable built with another adapter.
Install the resulting `e4shared` into the reference installation's `bin` directory
and pass the resulting `e4-nk-shared` to the comparison driver. The adapter
validates gzip checksums and lengths, supports concatenated members, and preserves
blank lines and final lines without a newline. `verify_gzip.py` checks interoperability
with Python's gzip implementation and rejection of damaged streams.

```sh
uv run --no-project python crates/ace-cfd/benchmarks/compare.py \
  --rust target/release/examples/sphere \
  --eilmer /path/to/eilmer-install \
  --eilmer-nk /path/to/e4-nk-shared \
  --case crates/ace-cfd/cases/sphere-viscous.json \
  --output /tmp/sphere-comparison
```

The output directory must not exist. Python needs only its standard library.
The default comparison uses `sphere-viscous.json` and 48×48, 96×96 and 192×192
cells; `--case`, `--resolutions` and `--steps` select the case, grid sequence and
iteration budget without changing the convergence thresholds.
The driver constructs identical mesh vertices, material, inflow and wall data, records
executable hashes and separates preparation from solver-process timing. It rejects
unconverged, oscillatory or noncontracting refinement sequences, excessive estimated
grid uncertainty, disagreeing extrapolated wall quantities and excessive differences
across the wall profile. This isotropic driver alone cannot qualify timing.
`compare_tensor_reference` requires complete directional families for both solvers
and retains both isotropic Richardson gates. Supply that qualified result as
`directional_verification` when using `time_to_tolerance` to select measured grids
meeting the same stagnation-heat and total-heating bounds. Timing also requires
qualified `boundary_verification`: Eilmer's native `OutFlowBC_Simple` copies
interior cell gradients to outlet faces, whereas Rust's zero-gradient modes remove
normal viscous derivatives. Native reference runs therefore provide boundary
sensitivity evidence until a common physical outlet condition has been verified.
A failed accuracy
gate must not be used to claim a performance advantage. This is a single timing
sample per grid; controlled repeated timings are needed for performance claims.
Newton–Krylov comparisons also repeat the finest reference solve from a converged
coarse-grid initial field. Its wall profile must reproduce the cold-start result
within the iterative heat-flux tolerance; different converged steady states reject
qualification even when their integrated heating agrees.
Omitting `--eilmer-nk` selects Eilmer's explicit local-time iteration.
For an interrupted Newton–Krylov solve, `eilmer_restart.prepare_restart_blocking`
copies the checkpoint and returns the `--snapshot-start` arguments. It preserves
the original saved residual reference and stopping tolerances. Loading a flow field
as a new initial condition instead defines a new reference residual and is a new
convergence experiment. The monitor checks the configured relative target and, for
prepared restarts, verifies that the original reference was retained.
Generated cases store flow snapshots in Eilmer's native binary format. For a saved
gzip-text benchmark, pass `binary_flow=True` to `prepare_restart_blocking`, or
`--binary-flow` to the trace command, to convert the copied snapshots to native-endian
binary64 values. This bypasses decimal-reader rounding while retaining the source
files and stopping controls. The converter supports the single-species,
single-temperature structured benchmark layout. Restart preparation checks the
selected payload against the time index and configured grid before copying it.
The Newton–Krylov driver independently re-evaluates the saved state with the same
residual thresholds; this verification is excluded from solver timing.
Use `--verification-only` when other calculations may contend for CPU time; it
retains the accuracy gates and disables qualification of a timing comparison.

To inspect a checkpoint without iteration, apply
`benchmarks/eilmer_patches/residual-trace.patch` to a separate Eilmer source copy
with `patch -p1 --fuzz=0` and build `e4-nk-shared` with the gzip adapter above.
`axis-regularity.patch` enforces zero radial interface velocity on the symmetry
axis before constructing viscous gradients. `shock-reconstruction.patch` makes
the configured shock suppression include a shock sensor equal to one.
`wall-temperature-gradient.patch` imposes the conductive normal derivative directly
from the fixed wall temperature and adjacent cell. It applies to laminar, single-temperature
fixed-temperature walls when gradients are calculated at cells.
Keep patched and unpatched reference executables separate; these corrections
change the discrete equations.
`continuous-sensor-jacobian.patch` includes the NNG sensor's variation when
evaluating Jacobian-vector products; it leaves the spatial residual unchanged.
`reconstruction-state-purity.patch` reconstructs velocity from local copies so
coordinate rotations cannot alter stored cell values. Apply `state-purity-trace.patch`
after `residual-trace.patch` and use `--require-state-purity` to require identical
cell states, gradients and residuals across repeated evaluations without time advancement.
Apply `constant-reconstruction.patch` after `reconstruction-state-purity.patch` to
preserve constant velocity through coordinate transformations and weighted interpolation.
Apply `constant-reconstruction-trace.patch` after the trace patches and use
`--require-constant-reconstruction` to check constant and affine fields through all
six stencils, in local and global coordinates.
Apply `binary-snapshot-rotation.patch` so snapshot rotation uses the configured
flow format. `restart-reference-roundtrip.patch` preserves binary64 restart scalars
and keeps the original reference file unchanged. `accepted-state-residual.patch`
refreshes the steady residual before stopping checks and writes loads from the
accepted state. These patches retain the stopping thresholds; use them together
with the residual and state-purity trace patches for saved-state verification.
Run `verify_eilmer_restart.py --source SAVED_CASE --executable E4_NK --installation DGD --output UNUSED_DIRECTORY`
to exercise native snapshot rotation and repeated restarts while retaining the stopping thresholds.
`outlet-zero-normal-gradient.patch` adds the explicit
`OutFlowBC_Simple:new{zeroNormalViscousGradients=true}` selection. It projects
velocity and temperature gradients onto the outlet tangent plane while retaining
tangential shear and axisymmetric strain. Native outlet behavior remains available.
This option requires laminar, single-species, single-temperature flow with cell
gradients. Use `verify_eilmer_outlet.py --source SAVED_CASE --output UNUSED_DIRECTORY
--executable INSTRUMENTED_BINARY --installation EILMER_INSTALL --case INPUT_JSON`
to verify the selected boundary on a checkpoint without advancing it. The check
compares against independent Newtonian stress and Fourier conduction formulas and
requires unchanged convection, cell states, cell gradients and non-outlet fluxes.
It checks the outlet only; the spatial and repeatability gates still apply.

```sh
uv run --no-project python crates/ace-cfd/benchmarks/trace_eilmer.py \
  --source /path/to/saved-eilmer-case --output /tmp/eilmer-trace \
  --executable /path/to/instrumented-e4-nk-shared \
  --installation /path/to/eilmer-install \
  --case crates/ace-cfd/cases/sphere.json \
  --require-axis-regularity --require-shock-suppression --require-wall-gradient
```

The trace copies the checkpoint, preserves its flow and residual-reference bytes,
and exports cell residuals, convective fluxes, total fluxes and viscous gradients.
It then checks the affine field `v_r = 1000 r` through Eilmer's boundary and gradient
routines, excluding boundary intersections. The command requires the original
case parameters and an unused output directory. Omit `--require-axis-regularity`
or the other `--require-*` flags to retain diagnostics from a failing reference.
Shock suppression is checked against neighbouring cell states at flagged internal
interfaces. The trace also exports an affine-strain residual, local linear and
quadratic temperature-gradient probes, a radial-temperature check through the full
wall-boundary pipeline, and isobaric thermodynamic reconstruction. Quadratic axial
and radial temperature fields expose conductive divergence at the axis. A pressure-balanced
affine strain field exports both momentum residuals and face fluxes, allowing point
quadrature and reconstruction errors to be distinguished. These consistency checks
do not qualify a steady solution, spatial convergence or performance.
For a case using the continuous NNG sensor, add
`--require-continuous-sensor-jacobian` to compare the actual complex-step derivative
with centered differences that recompute the sensor at each perturbation.
Use `--jacobian-outlet` with the selected zero-normal outlet to check a momentum
perturbation in an adjacent cell, excluding the wall intersection.

When simultaneous refinement hides opposing directional errors, a separate check
solves the full 3×3 grid family:

```sh
uv run --no-project python crates/ace-cfd/benchmarks/verify_directional.py \
  --rust target/release/examples/sphere --output /tmp/directional-verification
```

It requires converged runs and monotone, contracting radial and angular sequences
at both medium and fine transverse resolutions. It sums absolute directional and
mixed corrections, caps error-estimation orders at two and retains the 5% bound.
This evidence supplements the isotropic check; it does not change that check's
rejection rules or qualify a timing comparison.
`heat_convergence.py --solutions COARSE.json MEDIUM.json FINE.json --output AUDIT.json`
compares signed and absolute changes in wall-sector powers on nested uniform polar
grids. It diagnoses cancellation while retaining the original Richardson rejection.
Conservation closure and small changes in an integral do not establish spatial accuracy.

The spatial residual is separate from pseudo-time iteration. Fluxes use three
Cartesian momentum components and face normals, while this mesh and source operator
support only axisymmetry without swirl. `jacobian_vector` is a centered finite
difference for sensitivity verification, not an adjoint or automatic differentiation
implementation. Shock sensors and limiters are nonsmooth.

The self-authored component, examples, benchmark tooling and gzip adapter use
AGPL-3.0-only; see `LICENSE`. The rest of the workspace has its own license.
The external Eilmer patches
are GPL-3.0-or-later; their notices and license are in `benchmarks/eilmer_patches`.
The dependency guard checks declared
licenses across all target platforms, including example/test/build dependencies;
it does not audit operating-system libraries or the external Eilmer installation.
Eilmer is not a library or runtime dependency of the Rust solver. Gas properties and
meshes for these cases are generated without proprietary tables.

Numerical references: [AUSMDV and enthalpy-preserving shock dissipation](https://ntrs.nasa.gov/citations/19940032139),
[AUSM+-up](https://doi.org/10.1016/j.jcp.2005.09.020), and
[grid-convergence assessment](https://www.grc.nasa.gov/www/wind/valid/tutorial/spatconv.html).
The optional mesh guide uses [Billig's bow-shock shape](https://doi.org/10.2514/3.28969).
