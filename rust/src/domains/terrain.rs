use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::{TerrainOutput, TerrainParams};
use crate::common::geom::{
    add3, chord_distance, clamp, dot3, length3, lerp, mul3, normalize3, project_to_tangent, sub3,
};
use crate::common::mesh::{build_neighbors, generate_icosphere};
use crate::common::rng::{rng_from_seed, DeterministicRng};

include!("terrain/types.rs");
include!("terrain/noise.rs");
include!("terrain/plates.rs");
include!("terrain/boundaries.rs");
include!("terrain/surface.rs");
include!("terrain/pipeline.rs");
include!("terrain/tests.rs");
