# LLM-Workflow Evaluation: Four Aircraft, One Verdict

Evaluated 2026-07-24 by driving the CLI and MCP server end-to-end on four
airframes: the two shipped calibration examples (C172-class, B777-300ER-class)
and two deliberately out-of-envelope stress cases authored for this evaluation
(`examples/sr71/`, `examples/x15/`). All numbers below come from recorded runs.
The verdict, result table, and ranked flaws preserve the original evaluation
observations. The deterministic fixture contract records the guarded behavior
implemented since that evaluation.

**Original verdict:** the tool produces genuinely sensible designs inside its envelope
(C172 pass, 777 pass with ~15–20% fuel optimism), can fake one design point
outside it (SR-71 cruise closes only after lying to the schema in four
labeled places), and cannot represent the X-15 at all — but never says
"unsupported"; it either hard-crashes or silently produces garbage that passes
every gate. The ADRs (004/006/025) promise exactly the right things — explicit
validity, evidence envelopes, no silent substitution — and the implementation
fails to enforce them at the seams. The rework needed is not "add supersonic
models"; it is: make the envelope a first-class, queryable, statically-checked
object, and fix warning/metric plumbing so an eager LLM cannot be lied to.

## What each aircraft produced

| Aircraft | Outcome | Reality check |
|---|---|---|
| C172 | All hard requirements pass; soft ceiling requirement honestly fails by 3.7% | 86 kg fuel / 383 nmi ≈ 9.4 gph at 65% — right. Stall 45 kt, L/D 11.9 — right. |
| 777-300ER | All requirements pass, 7,490 nmi | Trip fuel 104.7 t is ~15–20% low; climb to FL350 takes 10 min / 2.8 t (real: ~22 min / ~8 t). The simplified climb model is the main error source. |
| SR-71 | Authentic mission (78 kft): hard crash. Clamped to 64 kft: "completed", all hard requirements pass | Cruise point right by construction (24 t/hr at M3.2 — matches). Everything off-design is garbage: subsonic leg burns 11.6 t/hr (~3× real), landed with 7 kg of fuel, no warning about either. The fixture also raises fuel capacity from NASA's 80,280 lb (36,414 kg) figure to 46,180 kg because the model cannot represent operational post-takeoff refueling. |
| X-15 | Authentic rocket profile rejected; a low-fidelity stand-in reports completion | The schema cannot represent an air launch or engine-off flight, and the simplified climb reaches 19,812 m in 17 s before the run produces an invalid passing result. |

## What works well for an LLM

- Determinism + content-addressed runs: same input, same hash, replayable.
- Stable-ID overrides (`mission.segments.<id>.<field>`) rather than array
  positions — robust to LLM file edits.
- Typed errors with stable codes; explicit units on every physical value; the
  assumptions-ledger concept.
- The atmosphere hard-refusal above 20 km — the one envelope boundary that
  enforces itself.
- The candidate-descriptor study model (no directory explosion, evidence by
  ID) is the right token-economics shape for agents.

## Deterministic fixture contract

The regression check consumes the fixtures through the public CLI. Both
the authentic SR-71 and X-15 scenarios fail during resolve-time model-domain
preflight with `MODEL_DOMAIN_UNSUPPORTED` and all known violating declaration
paths. A direct X-15 simulator regression separately exercises the
low-fidelity mission vocabulary: engine-off captive carry, glide, and landing
burn exactly zero fuel, retain their kinematics, inherit the explicit 45,000
ft initial state, and do not report `FUEL_EXHAUSTED`. Its energy-method boost
uses an ordered altitude/speed schedule and remains within 80–120 seconds; the
B777 climb remains within 15–30 minutes and 5–10 tonnes of fuel. Completed
missions publish bindable landing fuel and emit `LOW_LANDING_FUEL` below 5%
of maximum fuel capacity; incomplete missions emit neither.

## Original flaws, ranked by how badly they misled an LLM

1. **Strict mode is path-dependent.** `analyze point --strict` at M3.2 fails
   with `STRICT_WARNING_FAILURE` (polar extrapolation); the same aircraft at
   the same condition under `analyze mission --strict` returns
   `completed: true, hard_requirements_passed: true`. The mission integrator
   swallows segment-level model diagnostics before strict mode can see them.
2. **Requirement metrics are partly circular.** `performance.cruise_mach` is
   the declared mission Mach, not an achieved one — the X-15's "Mach ≥ 5.0"
   requirement passed with margin exactly 0.0% because 5.0 was copied from its
   own mission YAML. A requirement that can only fail if the mission crashes
   is a rubber stamp.
3. **Model boundaries are reported as physics.** SR-71 and X-15 both got
   `service_ceiling = 19,900 m` — the 20 km atmosphere cap minus solver
   margin, presented as `validity_status: valid` with no flag. The SR-71's
   80,000 ft ceiling requirement is unsatisfiable by construction and reads
   as an 18.4% design shortfall.
4. **Validation is structural, not physical.** A scenario cruising at
   78,000 ft against a 20 km atmosphere validates with zero warnings, then
   hard-fails at analysis. Everything needed to predict that failure is
   statically known at validate time.
5. **Wing geometry is over-determined and unreconciled.** `area`, `span`, and
   `aspect_ratio` are independent inputs; AR=20 with span/area implying 7.48
   is silently accepted and L/D jumps from 11.9 to 19.5. `auto_refine_design`
   enforces the planform identity; the general override path — and therefore
   studies, sweeps, and any LLM edit — does not.
6. **The propulsion vocabulary cannot say what it doesn't know.** Two profile
   types exist (piston, turbofan). Expressing the J58 required a negative
   Mach-lapse coefficient the schema accepted without comment; TSFC is one
   constant per mode, so off-design fuel flow is fiction with no diagnostic.
   "Validity" is the profile author's self-declared Mach/altitude box, not
   the calibration domain.
7. **The mission schema still lacks real flight phases.**
   There is no engine-off segment, mission initial state, or combined
   acceleration/climb segment, and legacy climb rate is unclamped. The X-15
   therefore cannot express its air launch or glide phases and reaches
   19,812 m in 17 seconds under the low-fidelity stand-in.
8. **Error and response ergonomics fight the agent.** MCP domain failures
   surface as JSON-RPC -32603 protocol errors, not `isError` tool results,
   losing path/context. CLI errors under `--format json` are Rust Debug text
   on stderr. One `simulate_mission` response is ~43 KB (~11k tokens),
   dominated by the inline assumptions ledger — which also parses free-text
   name fields as quantity+unit. A mission "completed" with 7 kg of fuel;
   display units ignore the project unit system.

## Rework direction

1. Envelope as data, checked at resolve time → `unsupported`, not crash or
   extrapolated numbers.
2. One diagnostics channel; strict mode must be path-independent (invariant:
   strict point failure ⇒ strict mission failure at the same condition).
3. Split declared vs. achieved metrics; requirements bind to achieved only.
4. Boundary-saturation flags (`boundary_limited`, requirement `indeterminate`).
5. Reconcile geometry at the resolver, every path.
6. Tabular propulsion (and Mach-dependent polar) decks: the data domain is
   the validity domain, so extrapolation is detectable rather than
   self-certified.
7. Mission vocabulary: engine-off, initial state, honored-or-rejected fields,
   energy-method climb/acceleration.
8. Agent-grade responses: compact by default, evidence by reference, `isError`
   tool results with actionable hints, a capability manifest so the valid
   vocabulary is discoverable without reading source.

**Acceptance metric for the whole effort:** the tool must never again say
"valid" about something it does not actually model — a hard `unsupported` is
cheap; a plausible wrong answer is poison.
