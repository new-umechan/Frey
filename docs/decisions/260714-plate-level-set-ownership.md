# Plate level-set ownership

## Status

Rejected

## Context

Generator influence converges toward centroidal Voronoi cells. A persistent material remap without
sub-cell interface reconstruction numerically diffuses mixed cells and fragments plate labels.
Frey needs an interface method that keeps the fixed spherical mesh and does not require a polygon
topology engine or PLIC/Moment-of-Fluid implementation.

References:

- Du, Emelianenko, and Ju (2006), https://doi.org/10.1137/040617364
- Dyadechko and Shashkov (2008), https://doi.org/10.1016/j.jcp.2007.12.029
- DeMets, Gordon, and Argus (2010), https://doi.org/10.1111/j.1365-246X.2009.04491.x

## Proposal

Represent each plate interface by a signed geodesic-distance field, but evolve a shared interface
rather than independently advecting overlapping plate regions. For each boundary edge, project one
boundary velocity onto the edge normal:

- ridge, rift, transform, collision, and passive boundaries use the mean of the two rigid velocities;
- subduction boundaries follow the overriding plate; rollback remains a future trench-velocity term.

Extend the shared velocity from the nearest boundary with edge-length Dijkstra, semi-Lagrangian
backtrace each signed-distance field through that velocity, then assign the largest evolved signed
distance. Unlike independent plate backtracing, both fields at an interface use the same boundary
velocity. This is a first-order spherical front approximation: tangential slip does not change
ownership, curved-front velocity extension is piecewise constant over nearest-boundary regions, and
deformation within a plate is omitted. Semi-Lagrangian transport avoids the mesh-CFL restriction of
an explicit signed-distance increment at `5 Myr/tick`.

Approximate signed geodesic distance and velocity extension with edge-length Dijkstra on the
icosphere. Do not conserve plate area globally. Ridge creation and subduction consumption are local
boundary processes, so a global area target has no physical basis and can reverse a local update.

Do not add connectivity repair, local majority voting, ownership hysteresis, generator relaxation,
or global area bias. Exact ties retain the current label only to avoid plate-ID ordering bias on the
discrete zero contour.

Initial kinematics must be generated in `km/Myr` and converted using `5 Myr/tick` and Earth radius.
Use a typical `20-90 km/Myr` sample band, with craton and subduction proxies, and cap initialization
at `110 km/Myr`. This fixes the previous direct `rad/tick` sampling that yielded an alpha mean near
`161 km/Myr`.

## Close when

Accept after zero-motion and common-rotation controls pass and alpha/beta/gamma/delta retain plate
count and shape through 160 ticks, followed by 400-tick long-run checks. Reject if uncaused splits,
reciprocal churn, or geometric regularization remain without adding topology repair.

## Outcome

Rejected on `seed=alpha`, level 6. Independent transport plus area correction reached 23 active
plates and `max_branch_area_ratio=0.58` by tick 160. Shared explicit-front and shared
semi-Lagrangian variants both exceeded 30 components in one plate by tick 5. A simple multiphase
`argmax` does not provide the regional constraints required to reconstruct one coherent interface;
adding label repair would hide that representation error.
