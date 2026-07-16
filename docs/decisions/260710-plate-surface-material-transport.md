# Plate surface material transport

## Status

Rejected

## Context

Independently rotating rigid regions create gaps and overlaps on a closed sphere. GPlates resolves
this with continuously closing polygons, while geodynamic solvers can advect composition or
particles. Frey tested a smaller transport-reaction-remap model on the fixed mesh.

References:

- Gurnis et al. (2012), https://doi.org/10.1016/j.cageo.2011.04.014
- Gerya and Yuen (2003), https://doi.org/10.1016/S0031-9201(03)00190-0
- ASPECT compositional fields,
  https://aspect-documentation.readthedocs.io/en/stable/parameters/Compositional_20fields.html

## Decision

Do not use surface material transport as runtime plate ownership.

The prototype rotated source dual cells, conservatively deposited overlap on fixed target cells,
and handled swept divergent/subduction regions. Local clipping used a gnomonic tangent plane and
normalized each source cell's overlap weights. These are explicit performance approximations, not a
global spherical polygon solution.

## Rationale

Analytic controls and one-tick alpha probes reached zero unresolved cells, so the remap primitives
remain useful research tools. Runtime alpha validation nevertheless reduced plate count from 9 to 4
and worsened block and weak-line stability. Boundary reaction also requires persistent subduction
initiation and a separate continental collision/obduction model; treating every convergent overlap
as crustal thickening is invalid.

Keep the probe and analytic tests, but do not add fallback reconstruction or tune reaction caps to
make this runtime path appear stable. Reconsider it only with an explicit plate lifecycle and
boundary-process state model.
