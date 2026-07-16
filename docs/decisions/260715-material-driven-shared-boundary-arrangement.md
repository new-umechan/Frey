# Material-driven shared boundary arrangement

## Status

Draft

## Context

Cell ownership, particles, and cell-local finite-volume reconstruction all fragmented under long
runs. Independently rotating closed plate polygons creates overlaps and gaps; resolving those by a
global plate priority or nearest centroid would introduce a new geometric attractor.

Continuously closing reconstruction instead builds each plate polygon from shared ridge, trench,
transform, and collision line sections. GPlates uses this separation and treats boundary sections
not shared by exactly two closed topologies as a coverage error.

References:

- Gurnis et al. (2012), https://doi.org/10.1016/j.cageo.2011.04.014
- GPlates topological closed plate polygons,
  https://www.gplates.org/docs/user-manual/topologytools/
- GPlates gap and overlap validation,
  https://www.gplates.org/docs/pygplates/sample-code/pygplates_detect_topology_gaps_and_overlaps
- Brochu and Bridson (2009), https://doi.org/10.1137/080737617
- Da et al. (2014), https://doi.org/10.1145/2601097.2601143

## Proposal

Keep one oriented spherical boundary graph shared by adjacent plates. For each step, rigidly advect
the material on both sides and derive one candidate boundary section from the local process:

- divergence places a ridge between the separated material fronts and creates young oceanic crust;
- subduction follows the overriding front and consumes only gated oceanic material;
- collision keeps a shared suture and records local crustal thickening;
- transform follows the mean tangential motion and changes no surface area.

Give all candidate sections to an S2 arrangement with crossing-edge splitting, then rebuild closed
plate polygons from the resulting shared sections. Intersections, edge collapse, plate loss, rift,
and suture are explicit topology events; raster cell ownership is only a sampled view of the closed
polygons. Do not apply global area caps, centroid attraction, connectivity repair, or plate-priority
gap filling.

The persistent authority is a spherical DCEL. Each segment owns a twin half-edge pair; each
half-edge stores `origin`, `twin`, `next`, `prev`, and face identity. A plate face may have multiple
boundary cycles, which represents holes without treating an enclosed plate as a disconnected host
plate. S2 arrangement output must retain its source half-edge identity. Raster labels and face
majority votes cannot rewrite DCEL connectivity.

Topology changes are local transactions over half-edges. Edge split preserves both incident faces;
edge collapse and triple-junction rewiring must produce closed `next`/`prev` cycles and preserve
two-sided incidence before commit. Plate split, merge, birth, or loss require separate lifecycle
transactions and cannot arise as a side effect of rasterization.

## Approximation

The process boundary is piecewise geodesic and one public tick is internally substepped. Ridge and
trench migration is represented by a rule-selected line between two rigid material fronts rather
than a deforming boundary zone. S2 resolves geometry robustly but does not choose the geological
meaning of a topology event; unsupported events must fail validation rather than be silently
relabelled.

## Close when

Require two-plate ridge, subduction, transform, and collision controls; triple-junction crossing and
edge-collapse controls; exact global coverage; each section shared twice; and no unexplained split
or plate loss. Then require alpha through 160 and 400 ticks and beta/gamma/delta through 160 without
persistent branch growth or increasing centroidal Voronoi agreement before visual review.

## Prototype result

Independent per-plate polygon arrangement was rejected because topology events switched authority
to raster labels. At alpha tick 40 it reached complexity 2.41 and a 42% secondary component.

A global S2 half-edge arrangement retained split-edge input labels and reconstructed exact faces
without raster remeshing. Initial faces had consistent labels, while the first known crossing was
confined to three mixed faces. Joint face labeling and an explicit three-plate degree-four flip
advanced alpha level 4 to tick 17. It then failed the half-edge plate-incidence invariant for every
candidate assignment. This is a fail-fast result, not a fallback.

The remaining design requires persistent half-edge `next`, `prev`, face identity, and event-local
transactions. Face labels alone cannot reconstruct those contracts after repeated events. Model 5
therefore remains experimental and is not ready for long-run or visual validation.

The persistent state and initialization now include twin half-edges, face boundary cycles, and
fail-fast DCEL validation. Hemisphere, triple-junction, enclosed-plate, and corrupted-link controls
pass. S2 split edges also retain directed source half-edge IDs. Runtime event transactions are not
yet adopted, so the decision remains Draft.

Local runtime variants were rejected before long-run validation. Segment-local process velocity
implicitly split a plate in the first update. Collision-safe rollback reached 4 constrained
segments at tick 1 and 88 at tick 2, so it converted the model error into a rapidly frozen front.
Plate-pair section rotation delayed but did not remove the same neck closure. Issuing a new plate ID
for the first split caused another unresolved split in the same update. Recomputing a triple
junction from rotated section great circles was unstable because three independently moved
sections do not generally share one intersection.

The next runtime implementation must treat all crossings in one junction neighbourhood as one
multimaterial event region. It must update section endpoints, half-edge connectivity, plate
lifecycle, and material accounting in one transaction. Per-crossing repair, motion clamping, and
raster-selected diagonals are rejected.

The incident-plate-mean hybrid initially stopped at alpha tick 11. Fixing batch rasterization and
general T1 transactions let it reach tick 40, but plate 2 developed a 0.42 secondary block. The
material-supported process-velocity variant then reached a junction annihilation at tick 9 where
every possible new edge was convergent. It remains fail-fast until plate loss/merge is an explicit
material lifecycle transaction and is not a long-run candidate.
