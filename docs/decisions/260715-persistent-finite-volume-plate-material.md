# Persistent finite-volume plate material

## Status

Rejected

## Context

Cell fractions, particles, and shared-boundary motion all lose information needed at convergence.
Re-expanding a mixed material fraction over its whole host cell diffuses the interface. Particles
leave sampling holes. Moving one shared line cannot resolve a physical overlap or gap without a
subduction, collision, or ridge event.

References:

- Scardovelli and Zaleski (1999), https://doi.org/10.1146/annurev.fluid.31.1.567
- Dyadechko and Shashkov (2008), https://doi.org/10.1016/j.jcp.2007.12.029
- Gurnis et al. (2012), https://doi.org/10.1016/j.cageo.2011.04.014

## Proposal

Triangulate the initial spherical dual cells and persist those triangles as Lagrangian material
elements. Rotate each element rigidly with its plate; never reconstruct its support from a mixed
Eulerian cell fraction. Clip elements into the fixed mesh only for field sampling and validation.

Treat projected area above one cell as overlap and below one cell as a gap. Resolve overlap locally:
oceanic material may subduct only through the age/density and convergent-boundary gate, while
continental overlap remains a collision stack until an explicit suture event. Fill only divergent
gaps with new young oceanic elements assigned from the incident ridge sides. Transform motion
creates neither material nor ownership transfer.

Visible cell ownership is a raster view of the upper material, not the persistent state. Therefore
cell fragments below one dual-cell area are raster diagnostics, while element connectivity and
support area determine physical split, branch, and plate survival.

## Approximation

Elements are piecewise-geodesic triangles and do not deform internally. Collision stacks omit
vertical rheology, and new ridge material is created at mesh resolution. This trades sub-cell
deformation for persistent support, rigid plate motion, and local material balance without global
area caps, centroid fields, or connectivity repair.

## Close when

Require identity, common rigid rotation, transform-only conservation, convergent overlap,
divergent gap, and raster-versus-element connectivity controls. Then require alpha through 160,
alpha through 400, and beta/gamma/delta through 160 without plate loss, persistent branch growth,
unsupported element split, or increasing centroidal Voronoi agreement.

## Outcome

The element transport and local overlap/gap closure controls passed. In the alpha level-4 control,
ridge creation closed 99.7% of divergent gap area and subduction removed 99.2% of convergent
overlap area. Identity, common rigid rotation, and differential rotation also conserved material.

The cell-local interface reconstruction was rejected. At alpha tick 20, boundary complexity reached
2.75 times its initial value, the largest plate had 14 raster blocks, and secondary blocks occupied
12.4% of a plate. A shared cut within each cell does not constrain the cuts on adjacent cells to
meet, so local conservation does not imply a continuous plate boundary. The transport and closure
controls remain useful, but finite-volume cells are not the ownership authority.
