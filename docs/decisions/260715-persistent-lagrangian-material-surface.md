# Persistent Lagrangian material surface

## Status

Draft

## Context

Conservative material transport avoids the centroidal Voronoi attractor, but rebuilding every
mixed dual cell loses interface continuity. Shared-boundary-only advection remains coherent for a
short run but fragments before 109 alpha level-4 ticks and cannot serve as the physical authority.

## Proposal

Persist the initial material triangles and rotate each triangle rigidly with its plate. Projection
onto fixed dual cells is a sampled view only; it must not expand a transported fragment over its
host cell or reconstruct the plate boundary.

At overlap, the boundary process selects the exposed material: gated subduction hides the
subducting oceanic side, while collision and transform retain the previous exposed side until an
explicit event changes it. At divergent gaps, only a traced ridge may expose newly accreted
oceanic material. A later transaction must add the exact gap polygon and remove the exact
subducted polygon from the persistent material set.

## Approximation

The first prototype samples triangle containment at mesh-cell centers and records unresolved
geometric source/sink work. It is not adoptable until ridge creation and subduction removal close
the persistent element coverage balance. Reject it early if rigid directional motion, plate
connectivity, or reciprocal churn is worse than the finite-volume and shared-boundary baselines.

Clipping fragments below `1e-5` of a mean dual-cell area are numerical dust: discard them and leave
their area visible in the residual gap balance. Larger unprojected fragments remain fatal errors.

Ridge accretion adds the uncovered area as an explicit gap phase to the local serial MoF partition.
Its area and first moment are the cell values minus projected material values. Only the resulting
gap support becomes new oceanic material. This avoids both the rejected global S2 Boolean backend
and combinatorial subtraction of overlapping persistent triangles.

Initial transported triangles are ownership markers. The current prototype has no persistent
shared ridge front, so leaving ridge-created material non-authoritative makes divergent gaps fall
back to label flood-fill. Making every MoF ridge fragment an independent marker was rejected: at
alpha level 4 tick 160 it raised the maximum block count from 4 to 10 and the maximum secondary
block ratio from 0.015 to 0.073. Letting ridge material fill only samples without an initial marker
was also rejected: it produced three blocks and a 0.014 secondary-block ratio by alpha tick 20.
Generated fragments therefore cannot act as ownership fronts without a shared interface.
