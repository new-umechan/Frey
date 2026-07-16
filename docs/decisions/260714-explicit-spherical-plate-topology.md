# Explicit spherical plate topology

## Status

Draft

## Context

Cell-wise ownership reconstruction has repeatedly traded boundary response for fragmentation. The
influence model also relaxes toward plate centroids, which creates a long-run centroidal Voronoi
attractor. Level-set, marker, and substepped remap prototypes did not preserve topology at Frey's
mesh resolution.

Plate reconstructions instead represent finite rotations and boundary topology separately. GPlates
uses this separation for evolving closed plate polygons and shared boundaries.

References:

- Gurnis et al. (2012), https://doi.org/10.1016/j.cageo.2011.04.014
- DeMets, Gordon, and Argus (2010), https://doi.org/10.1111/j.1365-246X.2009.04491.x
- S2 Geometry overview, https://s2geometry.io/about/overview.html
- `s2rst` 0.4 documentation, https://docs.rs/s2rst/0.4.0/s2rst/

## Proposal

Represent each plate boundary once as a spherical half-edge graph. Boundary vertices lie on mesh
edges; ordinary vertices have degree two and explicit triple junctions have degree three. Each
segment stores the plates on both sides. Euler rotations and boundary-process velocities move this
shared geometry, after which cell ownership is rasterized from closed regions.

Topology changes must be explicit events such as edge collapse, intersection rewiring, rifting, or
suture closure. They must not arise from independent cell relabeling. The first phase only extracts
the graph from current labels and verifies closed-boundary invariants. Runtime advection is not
adopted until extraction, round-trip rasterization, rigid-rotation, and long-run controls pass.

## Approximation

The graph is piecewise geodesic on the barycentric dual of the fixed spherical mesh. It does not
resolve faults below one cell, and it approximates continuous polygon intersections with local mesh
events. The trade-off is explicit shared topology without a full arbitrary-precision polygon engine.

## Close when

Adopt after alpha/beta/gamma/delta pass 160 ticks and the 400-tick runs show no persistent branches,
unexplained splits, open boundary endpoints, or increasing centroidal Voronoi agreement.

## Prototype result

The phase-one extractor is retained. It reconstructs one oriented shared boundary from mesh labels
and verifies degree-two boundary vertices, degree-three junctions, consistent plate pairs, and
closed per-plate half-edge incidence, including an enclosed plate.

Runtime advection is not adopted. Averaging incident plate rotations produced 14 segment
self-intersections on alpha after one tick. A globally interpolated Euler velocity reduced that to
2, but three simplified rasterizers still failed: spherical winding created disconnected labels,
nearest/swept classification created 13-22 blocks immediately, and mesh-edge barriers leaked closed
regions and reduced 9 plates to 3 in one tick. These paths were removed.

The next implementation must trace each moving boundary through mesh triangles and construct an
exact closed mesh cut, or use a spherical polygon arrangement engine. Cell-wise cleanup is not an
acceptable substitute for this missing geometry operation.

## S2 arrangement spike

Evaluate `s2rst` as the arrangement engine before writing another rasterizer. Persist the extracted
oriented half-edge graph, substep shared node motion below one mesh-edge width, and give the moved
geodesic edges to `S2Builder` for snapping and topological assembly. Rasterize only from the built
semi-open spherical regions, which must assign every cell center to exactly one plate. Do not use
boolean unions of independently moved plate polygons: shared edges remain the source of truth.

The spike is accepted only if native and `wasm32-unknown-unknown` builds pass, extraction followed
by S2 round-trip reproduces labels exactly, a common rigid rotation preserves all region topology,
and an alpha one-tick substep has no open edge, overlap, uncovered center, or plate loss.

## S2 spike result

Native and `wasm32-unknown-unknown` builds pass. Hemisphere, triple-junction, enclosed-plate, and
alpha level-6 extraction round-trip exactly through S2 polygons; a common rigid rotation also
preserves every alpha label. S2 therefore remains suitable as a validation and rasterization
backend.

Runtime advection still fails. With 32 internal substeps, incident-plate mean node motion makes the
first alpha loop invalid at substep 5, about 0.78 Myr into a public tick. S2 reports the crossing
rather than repairing it. Runtime adoption therefore depends on the component-local cut and
explicit topology-event design in `260714-component-local-conservative-boundary-cut.md`.

## Runtime spike result

A persistent S2-backed graph with adaptive segment subdivision preserved nine alpha plates through
40 ticks under a globally smooth Euler field. It was still rejected: boundary complexity reached
1.93 times the initial value and weak-line secondary blocks reached 9.8%. Narrowing the centroid
kernel did not converge under segment refinement and self-intersected near tick 30. The centroid
kernel is therefore not a physical ownership law.

Using adjacent-plate mean velocity removed the centroid dependency, but alpha required a local
triple-junction collapse within one public tick and then developed a separate loop collision. Such
events need material-aware overlap and gap resolution; suppressing them by velocity smoothing is
not adopted. The extractor, direct oriented-loop S2 validation, and remeshing controls remain useful
infrastructure, while runtime model 3 remains experimental.
