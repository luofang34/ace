# MVP acceptance criteria and evidence

The acceptance contract is executable. `./ci.sh` is the release gate.

1. **Both sample projects validate.** `cli_flows::validates_both_reference_projects`.
2. **CLI reports stall, drag, required/available power or thrust, climb,
   ceiling, fuel, and distance.** Point, performance, and mission commands plus
   calibration integration tests.
3. **C172 uses piston/propeller physics.** Typed profile resolution and
   propulsion lapse unit test.
4. **B777 uses turbofan thrust/TSFC physics.** Typed profile resolution and
   propulsion lapse unit test.
5. **Both examples produce reports, ledgers, warnings, curves, payload-range,
   and constraints.** CLI plot flows, immutable run layout, and sample
   artifacts.
6. **One- and two-dimensional sweeps work from CLI and MCP.** CLI 2-D
   integration test; `run_parameter_sweep` MCP tool uses the same service.
7. **CLI and MCP share numerical services.** Adapters contain no equations;
   stdio MCP integration invokes the same mission method tested by CLI.
8. **Every run has a manifest and stable content hash.** Run-store unit test
   and CLI integration run directories.
9. **Invalid units and physical values are structured failures.** Quantity and
   schema validation tests.
10. **Every analysis names model/version/fidelity/validity.** Result schemas and
    manifest assertions.
11. **Agent assumptions are visible.** The resolved ledger records provenance;
    strict mode recognizes `AGENT_ASSUMPTION`.
12. **The suite runs offline after dependencies are fetched.** `cargo test
    --all-targets` performs no network calls or external service calls.
13. **Native design exploration works without OpenVSP.** The MCP integration
    forces OpenVSP discovery to an absent executable, then creates, updates,
    and evaluates a C172-class design successfully.
14. **Backend-specific identifiers stay private.** MCP requests use canonical
    dotted scenario paths; OpenVSP component parameter names exist only in the
    subprocess adapter.
15. **OpenVSP adds refinement without replacing native feasibility.** The
    conditional installed-backend test asserts that native results remain the
    baseline while `.vsp3`, wetted-area, polar, and static pitching-moment data
    appear in a separate refinement.
16. **OpenVSP failure is explicit.** The adapter requires completion markers
    and usable structured outputs; process, API, and parse failures are typed
    backend errors with no fallback path.
17. **Automatic refinement has convergence guardrails.** The C172 refinement
    test requires all implemented requirements, structural screens, and
    mission operating-point power reserve to pass.
18. **Generated OpenVSP geometry has bounded proportions.** Script regression
    checks pin wing placement, fuselage sizing, and single vertical-tail
    symmetry; the installed-backend test bounds C172-class wetted area.
19. **Topology intent is explicit and capability checked.** C172 and B777
    graphs resolve and analyze natively, absent graphs preserve legacy native
    geometry, backend descriptors declare their supported vocabulary, and a
    canonical twin-boom graph returns an explicit unsupported result. Missing
    modeled tails, propulsion-count mismatches, rewired relationships, and
    unrepresented or unavailable OpenVSP features are rejected before backend
    execution or design creation.

## Calibration bands

Current deterministic reference behavior is checked against:

- C172 clean stall 45–60 kt, maximum level speed 120–150 kt, service ceiling
  11,000–16,000 ft, best glide ratio 7–12, payload-range near 500–800 nmi.
- B777 cruise Mach 0.82–0.85, service ceiling 39,000–45,000 ft, maximum
  payload 60–75 tonnes, long-range mission 6,500–8,000 nmi, takeoff T/W
  0.25–0.35, cruise L/D 16–22.

These are software-validation bands, not claims about certified aircraft.
