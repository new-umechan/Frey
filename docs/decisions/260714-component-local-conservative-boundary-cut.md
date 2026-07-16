# Component-local conservative boundary cut

## Status

Draft

## Context

Whole-surface material reconstruction either needs a non-local plate-area capacity or lets a
one-seed watershed change small-plate area by more than 100% per tick. Moving a shared polygon graph
avoids that reconstruction, but alpha develops its first loop intersection after only 5/32 of one
tick under incident-plate mean velocity. A robust polygon backend detects the event but cannot
choose its tectonic meaning.

References:

- Scardovelli and Zaleski (1999), https://doi.org/10.1146/annurev.fluid.31.1.567
- Dyadechko and Shashkov (2008), https://doi.org/10.1016/j.jcp.2007.12.029
- Gurnis et al. (2012), https://doi.org/10.1016/j.cageo.2011.04.014

## Proposal

Make the oriented shared boundary graph the ownership authority. Order its segments into stable
plate-pair components and retain each segment's containing mesh triangle. Within a substep, compute
one signed normal area flux for each shared cut. Apply equal and opposite occupancy changes to the
two sides, with ridge creation and subduction removal as explicit local sources and sinks.

Keep fractional cut position and residual area between ticks. Do not round to labels during
substeps, impose plate-area capacities, run a global watershed, or select one competing cell
proposal. Triple-junction pieces touching one triangle are committed as one local transaction;
negative occupancy, overlap, or a crossing retries with a smaller substep. Split, suture, collapse,
and intersection rewiring remain explicit lifecycle events.

Use the mean velocity of the two incident rigid plates as the unresolved interface velocity. This
fixes normal motion while treating tangential motion as a gauge. At triple junctions, apply a
short-range graph diffusion to the incident-plate velocity so adjacent components meet
continuously. Do not interpolate from global plate centroids; that introduces non-local deformation
unrelated to the adjacent boundary pair.

`s2rst` is a validation and rasterization backend, not the topology authority. It must reject an
invalid graph rather than normalize it into independently chosen plate polygons.

## Approximation

Cuts are geodesic inside fixed mesh triangles and conserve spherical area only to the triangle
quadrature accuracy. Plate deformation away from the boundary is omitted. Marker/material transport
may carry crust properties but cannot determine ownership.

## Close when

Require zero motion, common and inverse rigid rotation, transform-only motion, two-plate area flux,
simultaneous-component order independence, triple-junction closure, enclosed-plate preservation,
and local ridge/subduction controls before alpha 160/400 and multi-seed gates.
