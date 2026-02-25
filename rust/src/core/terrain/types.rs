#[derive(Clone)]
struct PlateAttr {
    is_ocean: bool,
    velocity: [f32; 3],
    base_height: f32,
    base_weight: f32,
}

#[derive(Clone, Copy)]
enum BoundaryType {
    Convergent,
    Divergent,
    Transform,
}

#[derive(Clone, Copy)]
enum ConvergentMode {
    OceanContinent,
    OceanOcean,
    ContinentContinent,
}

#[derive(Clone, Copy)]
enum SubductionPolarity {
    AUnderB,
    BUnderA,
    None,
}

#[derive(Clone, Copy)]
struct BoundaryEdge {
    a: usize,
    b: usize,
    plate_a: usize,
    plate_b: usize,
    boundary_type: BoundaryType,
    strength: f32,
    obliquity: f32,
}

#[derive(Clone, Copy)]
struct VertexLithosphere {
    age_norm: f32,
    weight: f32,
    buoyancy: f32,
}

struct BoundaryFields {
    preserve_strength: Vec<f32>,
    debug_trench_strength: Vec<f32>,
    debug_arc_strength: Vec<f32>,
    debug_backarc_strength: Vec<f32>,
    debug_ocean_ocean_arc_strength: Vec<f32>,
}

#[derive(Clone, Copy)]
struct BoundaryDistState {
    cost: f32,
    vertex: usize,
    source_edge: usize,
}

#[derive(Clone, Copy)]
struct QueueState {
    cost: f32,
    vertex: usize,
    plate: usize,
}

struct PlateGrowthProfile {
    spread: f32,
    preferred_axis: [f32; 3],
    secondary_axis: [f32; 3],
    axis_blend_axis: [f32; 3],
    anisotropy: f32,
    roughness: f32,
    warp_weights: [f32; 3],
    warp_gain: f32,
}

struct BoundaryVertices {
    mask: Vec<bool>,
    indices: Vec<usize>,
}

impl BoundaryVertices {
    fn new(len: usize) -> Self {
        Self {
            mask: vec![false; len],
            indices: Vec::new(),
        }
    }

    fn insert(&mut self, v: usize) {
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
