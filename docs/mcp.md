# MCP reference

Start the stdio server with `aex mcp serve`. The official Rust MCP SDK exposes
structured JSON output for:

- `validate_document`
- `list_profiles`
- `get_profile`
- `resolve_scenario`
- `calculate_point_performance`
- `simulate_mission`
- `generate_constraint_diagram`
- `generate_payload_range`
- `run_parameter_sweep`
- `compare_scenarios`
- `explain_result`
- `generate_report`

All scenario tools accept repository-relative or absolute paths. Overrides are
maps of dotted paths to explicit unit strings. Chart tools return a serializable
chart specification and may write an SVG only when an artifact path is
supplied. No MCP tool invokes a web service or language model.

