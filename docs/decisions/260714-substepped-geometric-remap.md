# Substepped geometric plate remap

## Status

Rejected

## Context

At level 6, one `5 Myr` tick moves initial plates roughly two to three mesh cells. Front, level-set,
and marker prototypes all reconstructed topology only after that full displacement and produced
overlap/gap ambiguity. Output cadence and numerical integration cadence need not be identical.

## Proposal

Reuse the existing spherical dual-cell overlap remap and connected material reconstruction. Split a
geology tick so maximum rigid displacement is at most 0.45 mean mesh-edge widths, scale Euler angular
speed by the substep count, and reconstruct after every substep. Apply the existing swept ridge and
subduction reactions with the same scaled kinematics.

This is a geometric remap approximation, not a new physical cap: all substeps integrate the full
Euler displacement. Reconstructing cell geometry each substep introduces numerical diffusion, while
avoiding a polygon topology engine. Monitor long-run boundary complexity and centroidal geometry for
that diffusion.

## Close when

Require zero-motion identity, common-rotation controls, higher net exchange directionality and lower
temporal reversal than the full-tick remap, alpha/beta/gamma/delta through 160 ticks, and 400-tick
non-Voronoi shape gates.

## Outcome

Rejected on `seed=alpha`, level 6. Shape remained connected through tick 40, but ownership was
effectively frozen: most ticks changed zero cells. Reconstructing a dominant label after every
sub-cell displacement discards the fractional motion before it can accumulate.
