# Local material interface reconstruction

## Status

Rejected

## Context

Persistent dual-cell material transport is conservative, but global plate-area capacities move
unrelated boundaries when mass overlaps at convergence or leaves gaps at divergence. Independent
first-moment cuts do not form a valid multiphase partition.

## Proposal

Rasterize transported material with one connected multi-source watershed. Material confidence is
the local cost; one previous-region seed per visible plate preserves identity. Do not impose a
plate-area target or capacity. Divergence and subduction must change material locally before
rasterization, and unresolved collision occupancy remains a model error rather than a remote area
correction.

This is a graph approximation to local interface reconstruction, not PLIC. It cannot represent a
physical split before the lifecycle model creates another seed. Reject it if a tick changes plate
area much faster than boundary-local transport permits, if plate count is lost, or if branch,
reversal, and Voronoi diagnostics regress.

## Close when

Require zero-motion, rigid common-rotation, boundary-locality, and connectedness controls; then run
alpha through 20 and 160 ticks before the multi-seed and 400-tick gates.

## Outcome

Alpha retained nine seeded IDs, but the largest per-tick plate-area change reached 48% by tick 5
and 207% by tick 20. Non-dominant assignments grew from 1,714 to 6,649 cells. Without a regional
interface constraint, a one-seed watershed can move a small plate boundary across unrelated mixed
material. Local unary costs alone are insufficient.
