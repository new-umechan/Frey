# Persistent marker surface material

## Status

Rejected

## Context

Cell labels, simplified material fractions, and mesh-edge fronts discard sub-cell interface
geometry. Frey already has quadrature parcels, spherical Euler transport, triangular projection,
and swept ridge/subduction reaction prototypes, but currently rebuilds parcels from labels for each
probe.

Marker-in-cell methods retain material identity on Lagrangian particles while solving fields on a
fixed Eulerian mesh; this is established in geodynamic simulations (Duretz et al. 2011,
https://doi.org/10.1029/2011GC003567). Conservative geometric remap would be preferable but requires
explicit interface reconstruction such as Moment-of-Fluid (Dyadechko and Shashkov 2008,
https://doi.org/10.1016/j.jcp.2007.12.029).

## Proposal

- Initialize persistent quadrature parcels once, with one center and one satellite per cell edge.
- Advect each parcel by its plate Euler rotation and project parcel mass to the fixed mesh.
- Create oceanic parcels along swept ridge/rift traces and remove oceanic parcels along swept
  subduction traces.
- Reconstruct the visible cell label from dominant projected material without connectivity repair,
  area caps, hysteresis, or generator fields.
- Reseed every 8 ticks from projected per-cell material fractions onto the same quadrature points.
  Normalize each surface cell to unit capacity; this prevents particle voids but introduces bounded
  remap diffusion and must be checked for long-run geometric regularization.
- Keep plate split/merge lifecycle disabled until material transport alone passes shape gates.

Reseeding preserves the local plate fractions represented by the projection, not the exact parcel
moments. Unit surface capacity is enforced locally; this is appropriate for an ownership/material
fraction but is not a crust-volume conservation law. Dominant-label smoothing is not used.

## Close when

Require zero-motion identity, common-rotation transport, alpha/beta/gamma/delta through 160 ticks,
then 400-tick shape and parcel-density gates. Reject if quadrature density still causes unsupported
fragments without an explicit conservative reseeding rule.

## Outcome

Rejected on `seed=alpha`, level 6. Without reseeding, uncovered cells exceeded the reconstruction
limit at tick 54. Eight-tick fraction-preserving reseeding prevented voids through tick 64, but
`net_exchange_directionality_ratio` reached 0.63, an improvement over older mutual exchange, but
`max_branch_area_ratio` reached 0.54. Persistent particles alone do not resolve the topology
ambiguity of reconstructing independently moved, overlapping plates after a multi-cell time step.
