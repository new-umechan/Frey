# Serial multi-material MoF reconstruction

## Status

Draft

## Context

The finite-volume material model avoids the centroidal Voronoi attractor, but its raster plate
field fragments. The current reconstruction partitions a mixed dual cell serially, yet fixes the
material order by area and points each cut directly at the transported centroid. That is a
first-moment heuristic, not Moment-of-Fluid reconstruction.

Dyadechko and Shashkov (2008), https://doi.org/10.1016/j.jcp.2007.12.029, reconstruct a
multi-material cell by comparing material orders and minimizing first-moment defects while
partitioning the remaining region. Frey can apply this local step before deciding whether a
persistent shared interface is still required.

## Proposal

For each mixed barycentric dual cell, try every material order up to four active materials. For
each extracted material, search the spherical cut direction that preserves its area and minimizes
the angular defect between the reconstructed and transported first moments. Assign the final
material the exact remaining polygon and choose the partition with the smallest total
area-weighted moment defect.

The reconstructed supports must form one exclusive partition: no independently reconstructed
material polygons, global plate capacity, connectivity flood, or ownership repair is allowed.
Raster ownership remains a diagnostic view of the dominant transported material.

## Limits

Cell-local MoF does not force cuts in adjacent cells to share endpoints. Adopt it only if control
tests reduce moment error and the alpha short run reduces fragmentation and reciprocal churn
without increasing centroidal Voronoi agreement. Otherwise retain material transport but move the
interface authority to the persistent shared-boundary arrangement.
