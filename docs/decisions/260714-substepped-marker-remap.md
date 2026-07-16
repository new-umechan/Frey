# Substepped marker remap

## Status

Rejected

## Context

Full-tick persistent markers preserve fractional material but reconstruct topology after a
multi-cell displacement. Substepped geometric remap controls displacement but rounds material to a
label after each substep. The controls show that both CFL-limited transport and fractional material
memory are required.

## Proposal

Transport persistent quadrature markers with Euler speed divided into substeps of at most 0.9 mean
mesh-edge widths. After every
substep, project plate fractions, reconstruct the connected visible surface, and reseed quadrature
markers from the projected fractions. Thus material fraction survives while marker density remains
uniform. Integrate the full `5 Myr` displacement; do not cap area or suppress reverse motion.

When a plate has no visible surface cell after reconstruction, retire its remaining surface
markers. A fully subducted slab may persist in a future mantle state, but must not reappear from the
surface ownership parcel pool.

The approximation is more diffusive and more expensive than a conservative PLIC/MoF remap. It is
acceptable only if long-run shape does not converge to compact geometric cells and runtime remains
practical.

## Close when

Require nonzero boundary response, high net exchange directionality, low same-cell temporal
reversal, alpha/beta/gamma/delta through 160 ticks, and 400-tick non-Voronoi and performance gates.

## Outcome

Rejected after the alpha 160-tick run. Plate count fell from 9 to 6, persistent branches appeared,
boundary complexity peaked at 2.10 times its initial value, and same-cell temporal reversal reached
0.74. Substepping improved short-run smoothness but did not preserve long-run topology.
