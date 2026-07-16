# Boundary front directional hysteresis

## Status

Rejected

## Context

Plate ownership transfer already accumulates fractional boundary-front motion by
`source_plate + target_plate + spatial bucket`.
This is not the same as stable boundary identity: the same boundary can still produce opposite
takeover proposals on nearby ticks, and the current `0.5` fractional rounding has no commit/release
hysteresis.

`reciprocal_churn_ratio` is also easy to misread.
It measures net one-way dominance across changed plate pairs, so it is not by itself a direct
"back-and-forth" metric.

## Decision

Add an explicit mutual-exchange diagnostic before changing the ownership rule.
Then test whether the existing boundary-front accumulator can be stabilized with a minimal
directional hysteresis:

- keep the existing bucket-level residual state
- require a full-cell progress threshold before committing the fractional remainder
- do not add persistent component IDs
- do not add progress smoothing in the first implementation
- keep topology guards and plate-level consistency projection unchanged

Material advection remains a preconditioner for now.
It is not folded into the hysteresis signal until the ownership-front change is measured on its own.

The first full-cell commit threshold experiment was rejected.
It reduced response to about `0.09-0.12` in the first two seeds, reduced straightness to about
`0.19-0.29`, and alpha exceeded the complexity gate.
The mutual-exchange diagnostic remains useful and is kept.

## Consequences

- A simple full-cell commit threshold is too blunt for this model.
- If this path is retried, it needs a softer reverse-debt state rather than replacing fractional
  commit with a hard full-cell gate.
- If shape metrics worsen, the change is rejected regardless of motion metrics.

## Validation

Run alpha/beta/gamma/delta level 6 for 160 ticks and compare:

- `mutual_exchange_ratio`
- `boundary_motion_response_ratio`
- `boundary_motion_runtime_actual_transfer_cell_count`
- `boundary_motion_runtime_budget_utilization_ratio`
- `max_plate_block_count`
- `max_boundary_complexity_growth`
- `max_abs_plate_area_delta_ratio`
- `max_weak_line_plate_block_count`
