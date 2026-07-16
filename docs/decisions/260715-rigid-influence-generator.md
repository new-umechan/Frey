# Rigid influence generator

## Status

Accepted

## Context

The default influence model relaxes every generator 20% toward its current region centroid each
tick. This is a spherical Lloyd step and therefore makes centroidal Voronoi geometry a long-run
attractor. Alpha at tick 400 appears increasingly geometric even though its short-run shapes pass.

## Proposal

Advance influence generators only by their plate Euler rotation. Retain local candidate competition,
area balance, ownership margin, backtrace support, and detached-component handling for the first
A/B test. This isolates centroid relaxation from the rest of the accepted model.

Use relaxation `0.10`. The zero, `0.02`, and `0.05` controls remain benchmark evidence only and
have no runtime model IDs.

## Close when

Require alpha/beta/gamma/delta through 160 ticks with no worse split or branch metrics than model 1,
then alpha through 400 ticks with lower centroidal Voronoi agreement, non-decreasing Voronoi energy
ratio, and acceptable boundary response and temporal reversal before visual review.

## Outcome

At level 6 and tick 160, relaxation `0.10` retained every plate and one actual component per plate.
Across alpha/beta/gamma/delta, Voronoi agreement fell from `0.84-0.92` to `0.77-0.87`, response was
`0.55-0.71`, and reversal was at most `0.038`. Alpha tick 400 retained nine plates and one component,
with Voronoi agreement `0.814`, energy ratio `0.612`, response `0.633`, and reversal `0.005`.

Beta weak-line block count increased from 3 to 5, so branch appearance remains a visual-review item.
