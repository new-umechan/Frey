# Plate material advection

## Status

Superseded

## Context

Superseded by `260708-plate-ownership-influence-field.md`.

Local boundary flux projection reduces fragmentary takeover, but `plate_id` is still updated by
discrete ownership transfer.
Because boundary components are rebuilt from the new rasterized `plate_id` every tick, boundaries can
look like they move back and forth even when Euler velocities are stable.

Frey should not implement a full spherical polygon topology engine.
The runtime is mesh-cell based, so the next model should keep the mesh but move plate material rather
than directly swapping ownership labels.

## Decision

Introduce a semi-Lagrangian plate material field on mesh cells.
Each cell stores a small sparse mixture of plate material weights.
At each Crust tick:

1. advect plate material using the source plate Euler velocity
2. reconstruct `plate_id` from the dominant material
3. keep boundary topology cleanup as validation/safety, not as the primary motion model

The first implementation uses a fixed small number of material slots per cell instead of particle
tracers.
This keeps runtime deterministic and close to the existing mesh data model.
The material field is used as a preconditioning motion model before the existing topology-safe
boundary cleanup, not as a topology-free replacement.

## Consequences

利点:

- `plate_id` が直接取り合うのではなく、material motion の結果として決まる
- rigid-like Euler motion と 5 Myr/tick の見た目が整合しやすい
- existing mesh, plate kinematics, validation, precompute pipeline を再利用できる

欠点:

- material mixing と dominant reconstruction の近似が必要
- plate boundary は rasterized material から復元するため、polygon model ほど鋭くはない
- crust age/thickness/density との coupling は段階的に調整する必要がある
- material-only reconstruction can fragment plates, so topology cleanup remains required
- this is not yet a conservative remap; material is used as a local motion signal

## Validation

alpha/beta/gamma/delta 160 tick run で最低限次を見る。

- `reciprocal_churn_ratio`
- `boundary_motion_response_ratio`
- `max_plate_block_count`
- `max_boundary_complexity_growth`
- `persistent_boundary_complexity_growth_plate_ratio`
- `max_weak_line_plate_block_count`
