# Plate material ownership redesign

## Status

Rejected

## Context

The default ownership model advances one generator per plate, relaxes it toward the current
centroid, and reclassifies cells by generator influence. This gives useful short-term motion but has
a centroidal Voronoi attractor. Alpha also shows visually separated lobes and branches that current
component and k-core diagnostics miss.

GPlates separates finite rotation from boundary topology, while marker-in-cell methods preserve
material identity during transport. Frey should keep its fixed spherical mesh but make transported
material, not a centroid generator, the ownership source.

References:

- Gurnis et al. (2012), https://doi.org/10.1016/j.cageo.2011.04.014
- Duretz et al. (2011), https://doi.org/10.1029/2011GC003567
- Du, Emelianenko, and Ju (2006), https://doi.org/10.1137/040617364

## Proposal

Separate runtime plate evolution into three layers:

1. persist per-cell plate-material mass and conservatively transport it by Euler rotation
2. change material only through explicit divergent, subduction, transform, and collision processes
3. create or retire plate IDs only through a persistent rift or suture lifecycle

Reuse the dual-cell overlap remap, but use dual-cell area mass and retain mixtures across ticks.
Do not reuse largest-component seeding, interface flood growth, or dominant-label reconstruction as
the material source of truth.

Before runtime integration, add multiscale neck, branch slenderness, and CVT-attraction diagnostics.
Zero-motion, common-rotation, and inverse-rotation controls must pass independently of seeded worlds.
Internal transport substeps may improve accuracy without changing the five-million-year public tick.

## Close when

Accept after the material model passes control tests and alpha/beta/gamma/eta at 400 and 800 ticks
without the current Voronoi attraction, uncaused persistent split, or branch regression. Reject if
conservative transport cannot avoid plate loss without topology-repair ownership rewrites.

## Outcome

The persistent dual-cell prototype conserved total material and passed zero-motion and rigid
round-trip controls. It nevertheless produced 10-67 dominant-label components per plate on alpha
within five ticks. The remap retained volume fractions but not sub-cell interface geometry, so each
mixed cell redistributed every material over the whole cell on the next tick.

A valid material implementation therefore requires PLIC or Moment-of-Fluid reconstruction with at
least per-material volume and centroid. That is larger than the current ownership scope. Model 3
instead evaluates the multiphase level-set design in
`260714-plate-level-set-ownership.md`.
