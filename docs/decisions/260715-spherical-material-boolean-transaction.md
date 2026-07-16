# Spherical material Boolean transaction

## Status

Rejected

## Context

Persistent material triangles preserve rigid plate motion and avoid the centroidal Voronoi
attractor. The current sampled exposure rule nevertheless leaves unsupported overlap and gap,
because it changes raster ownership without cutting the transported material support.

## Proposal

Use `s2rst` spherical Boolean operations to make each boundary update an area transaction:

- intersect transported material with the selected exposed face;
- retain the difference as buried or consumed material according to the boundary process;
- create only ridge-traced gap material;
- reject unsupported gap or overlap instead of assigning it to a nearby plate.

Apply operations in a boundary-local set of dual cells rather than unioning a whole plate each
tick. The raster remains an observation of the resulting material polygons, not their authority.

The first runtime slice is ridge accretion only: union transported triangles that intersect a
traced dual cell, subtract that support from the cell, and triangulate the exact gap. Projection to
the local tangent plane is used only for triangulation; the Boolean area transaction stays
spherical. Polygons with holes are rejected until their observed frequency justifies a hole-aware
tessellator.

## Outcome

Mesh controls preserved area, but the first alpha runtime call panicked inside `s2rst` with
`crossing count mismatch in process_edge2`. Runtime recovery would hide an invalid topology, so
this backend is rejected. Retain the boundary-local transaction design, but implement it with the
existing convex spherical clipper rather than `S2BooleanOperation`.

## Adoption gate

Before runtime integration, Boolean controls must preserve area for adjacent dual cells, shared
edges, containment, and Frey-sized transported triangle slivers. Results must be valid spherical
polygons. `s2rst` has a known edge-clipping regression, so a failed control rejects this backend;
it must not be hidden by tolerances or topology repair.
