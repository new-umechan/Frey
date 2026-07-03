use super::*;
use crate::sim::geology_types::{
    InitialPlateKinematics, PlateEmergenceFallbackKind, PlateId, TectonicRegime,
};

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
pub(super) enum EdgeReliefType {
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
    pub(super) boundary_type: EdgeReliefType,
    pub(super) strength: f32,
    pub(super) obliquity: f32,
    pub(super) convergence: f32,
    pub(super) divergence: f32,
    pub(super) transform: f32,
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
    pub(super) world_seed: String,
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
    pub(super) plate_emergence_regime: TectonicRegime,
    pub(super) plate_emergence_fallback: PlateEmergenceFallbackKind,
    pub(super) initial_plate_kinematics: Vec<InitialPlateKinematics>,
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
