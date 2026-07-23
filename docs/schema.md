# Schema and units

Documents are YAML or JSON mappings with `schema_version: 1` and one envelope:
`aircraft`, `mission`, `requirements`, `profile`, or `scenario`.

Physical values must include a unit:

```yaml
maximum_takeoff_mass: 1111 kg
area: 174 ft^2
cruise_speed: 122 kt
distance: 350 nmi
```

Bare numbers are reserved for dimensionless Mach, coefficients, ratios,
counts, and enumerated values. Unsupported or incompatible units return a
typed `Quantity`/validation error rather than guessing.

The resolver normalizes SI values and records scalar sources in an assumptions
ledger. Interface quantity objects contain `value`, `unit`, `display_value`,
and `display_unit`. Range fields use metres for `value` and nautical miles for
display by default.

