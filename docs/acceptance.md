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
    checks pin wing placement, fuselage sizing, mass-scaled engine envelopes,
    resolved propeller geometry, full-model CompGeom, and named VSPAERO lifting
    sets. Committed C172 and B777 top-view SVGs detect visual-layout drift; the
    installed-backend test bounds C172- and B777-class wetted areas.
19. **Topology intent is explicit and capability checked.** C172 and B777
    graphs resolve and analyze natively, absent graphs preserve legacy native
    geometry, backend descriptors declare their supported vocabulary, and a
    canonical twin-boom graph returns an explicit unsupported result. Missing
    modeled tails, propulsion-count mismatches, rewired relationships, and
    unrepresented or unavailable OpenVSP features are rejected before backend
    execution or design creation.
20. **Study definitions and evidence are portable and immutable.** C172 and
    B777 study YAML validates with its referenced scenario; default policies
    round-trip deterministically; equivalent candidate, evaluation, and
    archive content has the same identity while any material change has a new
    identity. Stored records are create-only, detect malformed JSON and
    content-ID corruption with path context, and never create candidate
    directories.
21. **Study execution is deterministic, resumable, and feasibility first.**
    C172 grid and B777 seeded evolutionary workflows return stable candidate
    order and selection; a truncated C172 archive resumes from 40 evaluations
    without duplicate evidence. Completed reruns reuse every evaluation.
    Ranking tests keep hard-infeasible candidates behind feasible candidates,
    and MCP load, run, query, chart, evidence retrieval, and single-design
    promotion agree on immutable identifiers. Conditional-grid duplicates do
    not consume the unique-candidate cap; reused study IDs require an archive
    discriminator; evidence links are cross-checked; and projected or
    unavailable charts are explicit.
22. **Resolved wing planforms are closed.** Any two of area, span, and aspect
    ratio derive the third; rounded triples are normalized through an inclusive
    0.5% tolerance and larger conflicts return
    `INCONSISTENT_WING_PLANFORM`. Direct overrides, all shipped examples,
    study candidates, sweep rows, and refinement outputs are regression tested
    against `aspect_ratio = span² / area`.
23. **Bounded solver results cannot masquerade as roots.** Interior envelope
    roots are valid or explicitly extrapolated, while atmosphere and default
    search-cap saturation is `boundary_limited`. Requirements bound to those
    values are indeterminate with nullable legacy `passed`; hard indeterminate
    constraints are infeasible and carry nonzero study violation.
24. **Cruise requirements use installed capability.** Declared cruise inputs
    remain reportable but cannot satisfy requirements. Achieved Mach/TAS and
    minimum excess power evaluate every cruise condition, aggregate at the
    weakest power condition, and make native feasibility false when any
    condition cannot close.

## Calibration bands

Current deterministic reference behavior is checked against:

- C172 clean stall 45–60 kt, maximum level speed 120–150 kt, service ceiling
  11,000–16,000 ft, best glide ratio 7–12, payload-range near 500–800 nmi.
- B777 achieved cruise Mach 0.82–0.85, service ceiling 39,000–45,000 ft, maximum
  payload 60–75 tonnes, long-range mission 6,500–8,000 nmi, takeoff T/W
  0.25–0.35, cruise L/D 16–22. Its energy climb completes in 15–30 minutes
  while consuming 5–10 tonnes of fuel.
- X-15 energy-climb boost duration is 80–120 seconds.

These are software-validation bands, not claims about certified aircraft.
