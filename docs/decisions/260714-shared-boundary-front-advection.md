# Shared boundary front advection

## Status

Rejected

## Context

Generator influence has a centroidal Voronoi attractor. Whole-region material and simplified
multiphase level-set remaps fragment labels without sub-cell interface geometry. Frey still needs a
fixed-mesh approximation that can resolve `5 Myr/tick` motion without independent cell takeover.

GPlates represents a plate boundary as shared topology rather than two independently reconstructed
regions (Gurnis et al. 2012, https://doi.org/10.1016/j.cageo.2011.04.014). MORVEL supplies the Euler
rotation basis for plate velocities (DeMets et al. 2010,
https://doi.org/10.1111/j.1365-246X.2009.04491.x).

## Proposal

Evolve ownership as a shared boundary front on mesh edges.

- Compute one absolute boundary velocity. Ridge, rift, transform, collision, and passive boundaries
  use the mean rigid velocity; a subduction trench follows the overriding plate.
- Project that velocity onto the edge normal. Its sign uniquely determines which side advances.
- Substep so normal displacement is at most 0.45 local edge widths.
- Accumulate fractional cell crossings on each current boundary cell and oriented plate pair.
- Commit all cells whose integrated front arrival reaches one cell width in the same substep. The
  velocity field supplies coherence; do not grow patches from coarse spatial buckets.
- Do not use global or plate-area caps.
- Recompute candidates after every substep. Tangential slip does not change ownership.

This is not a stress-resolving geodynamic model. Mean boundary velocity and cell-width arrival time
approximate explicit spherical boundary topology; the trade-off is confined to the ownership front.
Ridge migration, trench rollback, collision deformation, and plate lifecycle remain separately
testable process terms.

## Close when

Require zero-motion invariance, shared-direction tests, alpha/beta/gamma/delta through 160 ticks,
and 400-tick long runs without reciprocal churn, unsupported fragments, persistent branches, or a
centroidal Voronoi trend. Reject rather than add ownership smoothing if coherent fronts still fail.

## Outcome

Rejected on `seed=alpha`, level 6. Coarse component patches produced a growing toothed boundary and
about 41,000 transfers by tick 40. Per-cell arrival accumulation reduced the failure but still
reached 26 components in one plate and `reciprocal_churn_ratio=0.54`. Mesh-edge normals inherit the
stair-step label geometry; using that geometry as the next velocity normal amplifies its own error.
