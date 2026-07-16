# Capacitated material interface reconstruction

## Status

Rejected

## Context

The conservative dual-cell remap preserves transported plate material, but cell-wise dominant
labels lose sub-cell interfaces. A capacity-constrained connected flood preserved plate count and
connectivity on short alpha runs, but did not fix the transport representation. From tick 1 to 5,
closure assignments grew from 192 to 850 cells, capacity rebalancing from 151 to 711 cells, and
non-dominant assignments from 483 to 2,564 cells. Mean assigned material confidence fell from
0.980 to 0.914. Every material fraction was being redistributed over its whole host cell on the
next tick, so the interface diffused before graph reconstruction.

PLIC volume-of-fluid methods reconstruct an interface from conserved volume fractions rather than
reclassifying each cell independently. Frey does not yet retain the moments required for spherical
PLIC, so this decision tests a graph-based approximation before adding a full MoF state.

Reference:

- Scardovelli and Zaleski (1999), https://doi.org/10.1146/annurev.fluid.31.1.567

## Proposal

Persist spherical first moments together with each per-cell material mass. In a mixed source cell,
reconstruct a convex sub-cell support by cutting the barycentric dual polygon with a line whose
retained area matches the material fraction and whose direction follows the material centroid.
Rotate that support with its plate Euler rotation, conservatively clip it into target dual cells,
and accumulate both mass and first moment.

After transport, normalize plate mass into integer cell-area capacities. Seed each visible plate
from its highest-confidence cell in the previous region, then use connected multi-source growth and
topology-preserving boundary rebalancing to rasterize the transported interfaces.

This preserves plate identity and connectivity while material mass controls area. Previous labels
choose identity seeds only; they do not set the new boundary or area. Divergence and subduction
modify material before capacities are calculated.

## Approximation

The sub-cell reconstruction is a first-moment PLIC/MoF approximation, not a complete multiphase MoF
solver. Each material is reconstructed independently as one convex cut, so three-way cells can
contain overlap or void in their inferred supports; mass is still normalized conservatively during
remap. The graph rasterizer enforces one connected region per plate and cannot represent a physical
split until a lifecycle event creates another seed. Reject the approximation if confidence still
decays monotonically, closure/rebalancing grows without bound, or it creates branches, centroidal
attraction, plate loss, or material-area error beyond one cell per plate.

## Close when

Adopt only after zero-motion, common-rotation, diffuse-minority, and enclosed-plate controls pass;
then alpha/beta/gamma/delta pass 160 ticks and 400-tick runs do not trigger persistent branch,
fragmentation, temporal reversal, or centroidal Voronoi warnings.

## Outcome

The capacity rasterizer kept all nine alpha plates connected through tick 5, but temporal reversal
rose to 0.144 as closure and rebalancing grew every tick. Adding independent first-moment polygon
cuts reached reversal 0.171 and failed remap coverage at update 6. A global plate-area capacity
also transfers local convergence or divergence mass errors to unrelated boundaries. Use neither the
capacity correction nor the independent-cut MoF approximation.
