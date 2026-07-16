# Plate ownership influence field

## Status

Accepted

## Context

Cell-local transfer kept shapes connected, but ownership repeatedly exchanged across the same
boundary. Frey needs a mesh-native approximation of moving plate regions without a full spherical
polygon engine.

## Decision

Use a persistent spherical influence generator for each plate as the default ownership model.

- Advance each generator by the plate's Euler rotation with Rodrigues' formula.
- Relax it `0.1` toward the current region centroid to limit generator drift without making Lloyd
  relaxation dominate long-run geometry.
- Let only the current and adjacent plates compete for a cell.
- Score spherical proximity, initial-area balance (`0.18`), current ownership, neighbor support, and
  one-tick backtrace membership.
- Reassign only when the competing score exceeds a fixed margin.
- Until split/merge lifecycle exists, absorb every detached component into the adjacent plate with
  the greatest boundary contact.

This adapts centroidal Voronoi methods to a sphere and fixed mesh:

- Lloyd (1982), https://doi.org/10.1109/TIT.1982.1056489
- Du, Faber, and Gunzburger (1999), https://doi.org/10.1137/S0036144599352836
- Du, Ju, and Gunzburger (2003), https://doi.org/10.1137/S0036142903425410

## Consequences

This is not exact rigid-polygon advection. Relaxation and local reclassification deform boundaries.
One generator cannot represent a split, and component projection deliberately removes one.

Exact signed-distance backtrace, global moving Voronoi reconstruction, and conservative surface
material remap were rejected because they increased weak-line instability, replaced initial shapes
too abruptly, or reduced alpha from 9 to 4 plates.

## Validation

Relaxation `0.10` retains every initial plate and one actual component per plate through level-6
alpha/beta/gamma/delta tick 160. Alpha tick 400 retains nine plates and one component. The reduced
relaxation lowers long-run centroidal Voronoi agreement, while beta's weak-line block count remains
a visual-review risk. Full results are in the geology validation guide.
