# Bench Outer Layer

Simulation core and performance/validation tooling are separated by responsibility.

## Layer Mapping

1. API contract test
   - `pnpm run outer:contract`
   - validates WASM transport/API compatibility
2. Golden seed regression
   - `pnpm run outer:regression`
   - validates deterministic seed behavior against baseline
3. Module benchmark gate
   - `pnpm run outer:benchmark`
   - validates perf budget regression against perf baseline
4. Reference-data validation
   - `pnpm run outer:validation`
   - validates climate/hydrology quality/runtime against reference baselines

## Composite Gate

- `pnpm run outer:gate`
  - runs contract + regression in sequence
  - intended for stable CI quality gate at outer boundary
- `pnpm run outer:gate:full`
  - runs `outer:gate` + `outer:benchmark`
  - intended for manual/perf-focused full checks
