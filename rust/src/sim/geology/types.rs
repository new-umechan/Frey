use super::*;
use crate::sim::geology_types::PlateId;

#[derive(Clone)]
pub(super) struct PlateAttr {
    pub(super) is_ocean: bool,
    pub(super) velocity: [f32; 3],
    pub(super) drift_axis_primary: [f32; 3],
    pub(super) drift_axis_secondary: [f32; 3],
    pub(super) drift_mix_axis: [f32; 3],
    pub(super) drift_variability: f32,
    pub(super) base_height: f32,
    pub(super) base_weight: f32,
}

#[derive(Clone, Copy)]
pub(super) enum BoundaryType {
    Convergent,
    Divergent,
    Transform,
}

#[derive(Clone, Copy)]
pub(super) enum ConvergentMode {
    OceanContinent,
    OceanOcean,
    ContinentContinent,
}

#[derive(Clone, Copy)]
pub(super) enum SubductionPolarity {
    AUnderB,
    BUnderA,
    None,
}

#[derive(Clone, Copy)]
pub(super) struct BoundaryEdge {
    pub(super) a: usize,
    pub(super) b: usize,
    pub(super) plate_a: usize,
    pub(super) plate_b: usize,
    pub(super) boundary_type: BoundaryType,
    pub(super) strength: f32,
    pub(super) obliquity: f32,
}

#[derive(Clone, Copy)]
pub(super) struct VertexLithosphere {
    pub(super) age_norm: f32,
    pub(super) weight: f32,
    pub(super) buoyancy: f32,
    pub(super) competence: f32,
}

pub(super) struct BoundaryFields {
    pub(super) preserve_strength: Vec<f32>,
    pub(super) debug_trench_strength: Vec<f32>,
    pub(super) debug_arc_strength: Vec<f32>,
    pub(super) debug_backarc_strength: Vec<f32>,
    pub(super) debug_ocean_ocean_arc_strength: Vec<f32>,
}

#[derive(Clone, Copy)]
pub(super) struct BoundaryDistState {
    pub(super) cost: f32,
    pub(super) vertex: usize,
    pub(super) source_edge: usize,
}

#[derive(Clone, Copy)]
pub(super) struct QueueState {
    pub(super) cost: f32,
    pub(super) vertex: usize,
    pub(super) plate: usize,
}

pub(super) struct PlateGrowthProfile {
    pub(super) spread: f32,
    pub(super) preferred_axis: [f32; 3],
    pub(super) secondary_axis: [f32; 3],
    pub(super) axis_blend_axis: [f32; 3],
    pub(super) anisotropy: f32,
    pub(super) roughness: f32,
    pub(super) warp_weights: [f32; 3],
    pub(super) warp_gain: f32,
}

pub(super) struct BoundaryVertices {
    pub(super) mask: Vec<bool>,
    pub(super) indices: Vec<usize>,
}

impl BoundaryVertices {
    pub(super) fn new(len: usize) -> Self {
        Self {
            mask: vec![false; len],
            indices: Vec::new(),
        }
    }

    pub(super) fn insert(&mut self, v: usize) {
        if self.mask[v] {
            return;
        }
        self.mask[v] = true;
        self.indices.push(v);
    }
}

impl Ord for QueueState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for QueueState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for QueueState {}

impl PartialEq for QueueState {
    fn eq(&self, other: &Self) -> bool {
        self.vertex == other.vertex && self.plate == other.plate && self.cost == other.cost
    }
}

impl Ord for BoundaryDistState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for BoundaryDistState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for BoundaryDistState {}

impl PartialEq for BoundaryDistState {
    fn eq(&self, other: &Self) -> bool {
        self.vertex == other.vertex
            && self.source_edge == other.source_edge
            && self.cost == other.cost
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CrustUpdatePhase {
    InitMeshAndNoise,
    BuildPlateField,
    BuildBaseHeight,
    ApplyBoundaryRelief,
    ApplyCrustErosion,
    PostprocessSurface,
    ApplyHotspots,
    BuildHydrology,
    Done,
}

pub(crate) struct CrustTerrainUpdateState {
    pub(super) phase: CrustUpdatePhase,
    pub(super) params: GeologyParams,
    pub(super) rng: DeterministicRng,
    pub(super) positions: Vec<[f32; 3]>,
    pub(super) indices: Vec<u32>,
    pub(super) nbr_offsets: Vec<u32>,
    pub(super) nbrs: Vec<u32>,
    pub(super) spherical: Vec<(f32, f32)>,
    pub(super) phi: Vec<f32>,
    pub(super) plate_count_target: usize,
    pub(super) plate_id: Vec<PlateId>,
    pub(super) attributes: Vec<PlateAttr>,
    pub(super) boundary_edges: Vec<BoundaryEdge>,
    pub(super) vertex_lithosphere: Vec<VertexLithosphere>,
    pub(super) plate_boundary_proximity: Vec<f32>,
    pub(super) band_low: Vec<f32>,
    pub(super) band_mid: Vec<f32>,
    pub(super) band_high: Vec<f32>,
    pub(super) height: Vec<f32>,
    pub(super) boundary_fields: Option<BoundaryFields>,
    pub(super) river_flux: Vec<f32>,
    pub(super) river_next: Vec<i32>,
    pub(super) lake_depth: Vec<f32>,
}
