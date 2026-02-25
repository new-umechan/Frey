use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::{MeshOutput, TerrainOutput, TerrainParams};
use wasm_bindgen::prelude::*;

mod geom;
mod mesh;
mod rng;

use self::geom::{
    add3, chord_distance, clamp, dot3, length3, lerp, mul3, normalize3, project_to_tangent, sub3,
};
use self::mesh::{build_neighbors, flatten_positions, generate_icosphere};
use self::rng::{rng_from_seed, DeterministicRng};

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

pub(crate) fn generate_mesh(level: u32) -> Result<JsValue, JsValue> {
    if level > 8 {
        return Err(JsValue::from_str("level must be between 0 and 8"));
    }

    let (positions, indices) = generate_icosphere(level);
    let flattened_positions = flatten_positions(&positions);

    let output = MeshOutput {
        positions: flattened_positions,
        indices,
    };

    serde_wasm_bindgen::to_value(&output)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize mesh output: {err}")))
}

pub(crate) fn generate_terrain(seed: String, params_js: JsValue) -> Result<JsValue, JsValue> {
    let mut params = if params_js.is_undefined() || params_js.is_null() {
        TerrainParams::default()
    } else {
        serde_wasm_bindgen::from_value::<TerrainParams>(params_js)
            .map_err(|err| JsValue::from_str(&format!("invalid terrain params: {err}")))?
    };

    sanitize_params(&mut params);

    if seed == "earth" {
        let (positions, indices) = generate_icosphere(params.level);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let preset = earth_preset(&positions, &nbr_offsets, &nbrs, params.river_rain_base);
        return serde_wasm_bindgen::to_value(&preset).map_err(|err| {
            JsValue::from_str(&format!("failed to serialize terrain output: {err}"))
        });
    }

    let mut rng = rng_from_seed(&seed, &params);

    let (positions, indices) = generate_icosphere(params.level);
    let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
    let spherical = compute_spherical_coords(&positions);
    let mut phi = evaluate_phi(&spherical, params.l_max, params.alpha, &mut rng);
    normalize_zscore(&mut phi);

    let plate_count = choose_plate_count(params.num_plates_min, params.num_plates_max, &mut rng);
    let seeds = pick_plate_seeds(&phi, &positions, &nbr_offsets, &nbrs, plate_count, &mut rng);
    let growth_profiles = build_plate_growth_profiles(plate_count, &mut rng);
    let plate_cost_warp_basis =
        generate_plate_cost_warp_basis(positions.len(), &nbr_offsets, &nbrs, &mut rng);
    let mut plate_id = partition_plates(
        &positions,
        &phi,
        &plate_cost_warp_basis,
        &nbr_offsets,
        &nbrs,
        &seeds,
        &growth_profiles,
        params.boundary_band,
    );
    plate_id = compact_plate_ids(plate_id, plate_count);
    cleanup_plate_components(&nbr_offsets, &nbrs, &mut plate_id, plate_count);
    plate_id = compact_plate_ids(plate_id, plate_count);
    let attributes = assign_plate_attributes(plate_count, &mut rng, params.ocean_plate_ratio);
    let boundary_edges =
        extract_boundary_edges(&positions, &nbr_offsets, &nbrs, &plate_id, &attributes);
    let vertex_lithosphere = compute_vertex_lithosphere(
        &positions,
        &nbr_offsets,
        &nbrs,
        &plate_id,
        &attributes,
        &boundary_edges,
    );
    let plate_boundary_proximity =
        compute_plate_boundary_proximity(&nbr_offsets, &nbrs, &plate_id, 3);
    let (band_low, band_mid, band_high) = generate_frequency_bands(
        &spherical,
        &nbr_offsets,
        &nbrs,
        params.l_max,
        params.alpha,
        &mut rng,
    );

    let mut height = vec![0.0; positions.len()];
    for v in 0..positions.len() {
        let pid = plate_id[v] as usize;
        let boundary_w = plate_boundary_proximity[v];
        let land_ocean_scale = if attributes[pid].is_ocean { 0.85 } else { 1.0 };
        let low_amp = 0.12;
        let mid_amp = lerp(0.045, 0.085, boundary_w) * land_ocean_scale;
        let high_amp = lerp(0.010, 0.030, boundary_w) * land_ocean_scale;
        let jitter = rng.gen_range_f32(-0.008, 0.008);
        let crust_base = if attributes[pid].is_ocean {
            vertex_lithosphere[v].buoyancy
        } else {
            attributes[pid].base_height
        };
        height[v] = clamp(
            crust_base
                + 0.08 * phi[v]
                + low_amp * band_low[v]
                + mid_amp * band_mid[v]
                + high_amp * band_high[v]
                + jitter,
            -1.2,
            1.2,
        );
    }

    let boundary_fields = apply_boundary_model(
        &positions,
        &nbr_offsets,
        &nbrs,
        &plate_id,
        &attributes,
        &vertex_lithosphere,
        &boundary_edges,
        &mut height,
        &params,
    );

    smooth_heights(
        &nbr_offsets,
        &nbrs,
        &boundary_fields,
        &mut height,
        params.smooth_iter,
        params.smooth_lambda,
    );

    apply_hydraulic_erosion(&positions, &nbr_offsets, &nbrs, &mut height, &params);

    postprocess_height(
        &nbr_offsets,
        &nbrs,
        &mut height,
        clamp(params.ocean_plate_ratio + 0.04, 0.55, 0.78),
    );

    let (river_flux, river_next) = generate_rivers(
        &positions,
        &nbr_offsets,
        &nbrs,
        &height,
        params.river_rain_base,
        params.river_accum_threshold,
    );
    let vertex_weight = vertex_lithosphere
        .iter()
        .map(|lith| lith.weight)
        .collect::<Vec<_>>();
    let plate_is_ocean = attributes
        .iter()
        .map(|attr| u8::from(attr.is_ocean))
        .collect::<Vec<_>>();
    let plate_base_height = attributes
        .iter()
        .map(|attr| attr.base_height)
        .collect::<Vec<_>>();
    let plate_base_weight = attributes
        .iter()
        .map(|attr| attr.base_weight)
        .collect::<Vec<_>>();

    let output = TerrainOutput {
        height,
        plate_id,
        river_flux,
        river_next,
        vertex_weight,
        plate_is_ocean,
        plate_base_height,
        plate_base_weight,
    };

    serde_wasm_bindgen::to_value(&output)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize terrain output: {err}")))
}

fn sanitize_params(params: &mut TerrainParams) {
    params.level = params.level.min(8);
    params.l_max = params.l_max.max(2).min(8);
    params.alpha = params.alpha.max(0.1);
    if params.num_plates_min < 2 {
        params.num_plates_min = 2;
    }
    if params.num_plates_max < params.num_plates_min {
        params.num_plates_max = params.num_plates_min;
    }
    params.ocean_plate_ratio = clamp(params.ocean_plate_ratio, 0.0, 1.0);
    params.boundary_band = params.boundary_band.max(1e-3);
    params.boundary_convergent_base_gain = params.boundary_convergent_base_gain.max(0.0);
    params.boundary_divergent_base_gain = params.boundary_divergent_base_gain.max(0.0);
    params.boundary_transform_relief_gain = params.boundary_transform_relief_gain.max(0.0);
    params.trench_gain = params.trench_gain.max(0.0);
    params.arc_gain = params.arc_gain.max(0.0);
    params.collision_gain = params.collision_gain.max(0.0);
    params.rift_gain = params.rift_gain.max(0.0);
    params.boundary_width_trench = params.boundary_width_trench.max(1e-3);
    params.boundary_width_arc = params.boundary_width_arc.max(1e-3);
    params.boundary_width_collision = params.boundary_width_collision.max(1e-3);
    params.boundary_width_rift = params.boundary_width_rift.max(1e-3);
    params.boundary_obliquity_mix = clamp(params.boundary_obliquity_mix, 0.0, 1.0);
    params.boundary_distance_falloff = params.boundary_distance_falloff.max(0.1);
    params.boundary_anisotropy = clamp(params.boundary_anisotropy, 0.0, 1.0);
    params.smooth_lambda = clamp(params.smooth_lambda, 0.0, 1.0);
    params.river_rain_base = params.river_rain_base.max(0.0);
    params.river_accum_threshold = params.river_accum_threshold.max(0.0);
    params.erosion_iter = params.erosion_iter.min(128);
    params.hydraulic_erode_rate = params.hydraulic_erode_rate.max(0.0);
    params.hydraulic_deposit_rate = clamp(params.hydraulic_deposit_rate, 0.0, 1.0);
    params.sediment_capacity_gain = params.sediment_capacity_gain.max(0.0);
    params.erosion_min_slope = params.erosion_min_slope.max(0.0);
    params.erosion_max_delta_per_iter = params.erosion_max_delta_per_iter.max(0.0);
    params.coastal_deposit_rate = clamp(params.coastal_deposit_rate, 0.0, 1.0);
    params.shallow_sea_floor = clamp(params.shallow_sea_floor, -1.0, 0.0);
}

fn compute_spherical_coords(positions: &[[f32; 3]]) -> Vec<(f32, f32)> {
    positions
        .iter()
        .map(|p| {
            let theta = clamp(p[1], -1.0, 1.0).acos();
            let lambda = p[2].atan2(p[0]);
            (theta, lambda)
        })
        .collect()
}

fn evaluate_phi(
    spherical: &[(f32, f32)],
    l_max: u32,
    alpha: f32,
    rng: &mut DeterministicRng,
) -> Vec<f32> {
    let mut coeffs: Vec<Vec<f32>> = Vec::with_capacity((l_max + 1) as usize);
    coeffs.push(vec![0.0]);
    coeffs.push(vec![0.0, 0.0, 0.0]);

    for l in 2..=l_max {
        let sigma = 1.0 / (l as f32).powf(alpha);
        let len = (2 * l + 1) as usize;
        let mut arr = vec![0.0; len];
        for value in &mut arr {
            let z = rng.standard_normal();
            *value = sigma * z;
        }
        coeffs.push(arr);
    }

    let mut phi = vec![0.0; spherical.len()];
    for (i, (theta, lambda)) in spherical.iter().enumerate() {
        let mut sum = 0.0;
        for l in 2..=l_max {
            for m in -(l as i32)..=(l as i32) {
                let c = coeffs[l as usize][(m + l as i32) as usize];
                sum += c * real_spherical_harmonic(l as i32, m, *theta, *lambda);
            }
        }
        phi[i] = sum;
    }
    phi
}

fn real_spherical_harmonic(l: i32, m: i32, theta: f32, lambda: f32) -> f32 {
    let abs_m = m.abs();
    let x = theta.cos();
    let p_lm = associated_legendre(l, abs_m, x);

    let normalization = (((2 * l + 1) as f32 / (4.0 * std::f32::consts::PI))
        * factorial((l - abs_m) as u32)
        / factorial((l + abs_m) as u32))
    .sqrt();

    if m > 0 {
        (2.0_f32).sqrt() * normalization * p_lm * (abs_m as f32 * lambda).cos()
    } else if m < 0 {
        (2.0_f32).sqrt() * normalization * p_lm * (abs_m as f32 * lambda).sin()
    } else {
        normalization * p_lm
    }
}

fn associated_legendre(l: i32, m: i32, x: f32) -> f32 {
    if m > l {
        return 0.0;
    }

    let mut p_mm = 1.0;
    if m > 0 {
        let root = (1.0 - x * x).max(0.0).sqrt();
        let mut factor = 1.0;
        for _ in 1..=m {
            p_mm *= -factor * root;
            factor += 2.0;
        }
    }

    if l == m {
        return p_mm;
    }

    let p_m1m = x * (2 * m + 1) as f32 * p_mm;
    if l == m + 1 {
        return p_m1m;
    }

    let mut p_prev = p_mm;
    let mut p_curr = p_m1m;

    for ll in (m + 2)..=l {
        let p_next =
            (((2 * ll - 1) as f32) * x * p_curr - ((ll + m - 1) as f32) * p_prev) / (ll - m) as f32;
        p_prev = p_curr;
        p_curr = p_next;
    }

    p_curr
}

fn factorial(n: u32) -> f32 {
    if n <= 1 {
        return 1.0;
    }
    (2..=n).fold(1.0, |acc, v| acc * v as f32)
}

fn normalize_zscore(data: &mut [f32]) {
    let mean = data.iter().sum::<f32>() / data.len() as f32;
    let variance = data
        .iter()
        .map(|v| {
            let d = *v - mean;
            d * d
        })
        .sum::<f32>()
        / data.len() as f32;
    let std = variance.sqrt().max(1e-6);

    for v in data {
        *v = (*v - mean) / std;
    }
}

fn normalize_zscore_if_var(data: &mut [f32]) {
    if data.is_empty() {
        return;
    }
    let mean = data.iter().sum::<f32>() / data.len() as f32;
    let variance = data
        .iter()
        .map(|v| {
            let d = *v - mean;
            d * d
        })
        .sum::<f32>()
        / data.len() as f32;
    if variance < 1e-8 {
        data.fill(0.0);
        return;
    }
    let std = variance.sqrt();
    for v in data {
        *v = (*v - mean) / std;
    }
}

fn generate_frequency_bands(
    spherical: &[(f32, f32)],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    l_max: u32,
    alpha: f32,
    rng: &mut DeterministicRng,
) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let low_max = l_max.max(3).min(5);
    let mid_max = (l_max + 3).max(low_max + 1).min(10);

    let mut low = evaluate_phi_band(spherical, 2, low_max, alpha + 0.35, rng);
    let mut mid = evaluate_phi_band(spherical, low_max + 1, mid_max, alpha, rng);
    if mid.iter().all(|v| v.abs() < 1e-7) {
        mid = generate_smoothed_noise_band(spherical.len(), nbr_offsets, nbrs, 3, 1, rng);
    }

    let mut high = generate_smoothed_noise_band(spherical.len(), nbr_offsets, nbrs, 1, 4, rng);

    normalize_zscore_if_var(&mut low);
    normalize_zscore_if_var(&mut mid);
    normalize_zscore_if_var(&mut high);

    (low, mid, high)
}

fn evaluate_phi_band(
    spherical: &[(f32, f32)],
    l_min: u32,
    l_max: u32,
    alpha: f32,
    rng: &mut DeterministicRng,
) -> Vec<f32> {
    if l_min > l_max {
        return vec![0.0; spherical.len()];
    }

    let mut coeffs: Vec<Vec<f32>> = vec![Vec::new(); (l_max + 1) as usize];
    for l in l_min..=l_max {
        let sigma = 1.0 / (l as f32).powf(alpha.max(0.1));
        let len = (2 * l + 1) as usize;
        let mut arr = vec![0.0; len];
        for value in &mut arr {
            *value = sigma * rng.standard_normal();
        }
        coeffs[l as usize] = arr;
    }

    let mut out = vec![0.0; spherical.len()];
    for (i, (theta, lambda)) in spherical.iter().enumerate() {
        let mut sum = 0.0;
        for l in l_min..=l_max {
            for m in -(l as i32)..=(l as i32) {
                let c = coeffs[l as usize][(m + l as i32) as usize];
                sum += c * real_spherical_harmonic(l as i32, m, *theta, *lambda);
            }
        }
        out[i] = sum;
    }
    out
}

fn generate_smoothed_noise_band(
    count: usize,
    nbr_offsets: &[u32],
    nbrs: &[u32],
    smooth_short: u32,
    smooth_long: u32,
    rng: &mut DeterministicRng,
) -> Vec<f32> {
    let mut raw = vec![0.0; count];
    for v in &mut raw {
        *v = rng.gen_range_f32(-1.0, 1.0);
    }

    let mut a = raw.clone();
    let mut b = raw;
    smooth_scalar_field(nbr_offsets, nbrs, &mut a, smooth_short);
    smooth_scalar_field(nbr_offsets, nbrs, &mut b, smooth_long.max(smooth_short));

    a.iter().zip(b.iter()).map(|(x, y)| x - y).collect()
}

fn smooth_scalar_field(nbr_offsets: &[u32], nbrs: &[u32], field: &mut [f32], iter: u32) {
    if iter == 0 || field.is_empty() {
        return;
    }
    let mut buf = field.to_vec();
    for _ in 0..iter {
        for v in 0..field.len() {
            let start = nbr_offsets[v] as usize;
            let end = nbr_offsets[v + 1] as usize;
            if start == end {
                buf[v] = field[v];
                continue;
            }
            let mut sum = field[v];
            let mut wsum = 1.0;
            for &n in &nbrs[start..end] {
                sum += field[n as usize];
                wsum += 1.0;
            }
            buf[v] = sum / wsum;
        }
        field.copy_from_slice(&buf);
    }
}

fn compute_plate_boundary_proximity(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    max_hops: u32,
) -> Vec<f32> {
    let mut dist = vec![u32::MAX; plate_id.len()];
    let mut frontier = Vec::<usize>::new();

    for v in 0..plate_id.len() {
        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;
        for &n in &nbrs[start..end] {
            if plate_id[v] != plate_id[n as usize] {
                dist[v] = 0;
                frontier.push(v);
                break;
            }
        }
    }

    let mut head = 0usize;
    while head < frontier.len() {
        let v = frontier[head];
        head += 1;
        let d = dist[v];
        if d >= max_hops {
            continue;
        }
        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;
        for &n in &nbrs[start..end] {
            let n = n as usize;
            if dist[n] > d + 1 {
                dist[n] = d + 1;
                frontier.push(n);
            }
        }
    }

    dist.iter()
        .map(|&d| {
            if d == u32::MAX {
                0.0
            } else {
                (1.0 - d as f32 / (max_hops.max(1) as f32 + 1.0)).max(0.0)
            }
        })
        .collect()
}

fn choose_plate_count(min_count: u32, max_count: u32, rng: &mut DeterministicRng) -> usize {
    if min_count == max_count {
        min_count as usize
    } else {
        rng.gen_range_u32_inclusive(min_count, max_count) as usize
    }
}

fn pick_plate_seeds(
    phi: &[f32],
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_count: usize,
    rng: &mut DeterministicRng,
) -> Vec<usize> {
    let mut max_candidates = Vec::<usize>::new();
    let mut min_candidates = Vec::<usize>::new();

    for v in 0..phi.len() {
        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;

        let mut is_max = true;
        let mut is_min = true;

        for &n in &nbrs[start..end] {
            let n = n as usize;
            if phi[v] <= phi[n] {
                is_max = false;
            }
            if phi[v] >= phi[n] {
                is_min = false;
            }
            if !is_max && !is_min {
                break;
            }
        }

        if is_max {
            max_candidates.push(v);
        }
        if is_min {
            min_candidates.push(v);
        }
    }

    max_candidates.sort_by(|a, b| phi[*b].partial_cmp(&phi[*a]).unwrap_or(Ordering::Equal));
    min_candidates.sort_by(|a, b| phi[*a].partial_cmp(&phi[*b]).unwrap_or(Ordering::Equal));

    let k_up = plate_count / 2;
    let k_down = plate_count - k_up;

    let mut seeds = Vec::<usize>::with_capacity(plate_count);
    let mut min_spacing = estimate_seed_min_spacing(plate_count);
    for _ in 0..5 {
        take_spaced_candidates(&mut seeds, &max_candidates, positions, k_up, min_spacing);
        take_spaced_candidates(&mut seeds, &min_candidates, positions, k_up + k_down, min_spacing);
        if seeds.len() >= plate_count {
            break;
        }
        min_spacing *= 0.82;
    }

    while seeds.len() < plate_count {
        let next = farthest_point_seed(positions, &seeds, rng);
        if !seeds.contains(&next) && is_seed_far_enough(positions, &seeds, next, min_spacing * 0.7) {
            seeds.push(next);
        } else {
            break;
        }
    }

    if seeds.is_empty() {
        seeds.push(rng.gen_range_usize(0, phi.len()));
    }

    while seeds.len() < plate_count {
        let mut next = rng.gen_range_usize(0, phi.len());
        let mut attempts = 0;
        while attempts < 12 && (seeds.contains(&next) || !is_seed_far_enough(positions, &seeds, next, min_spacing * 0.45))
        {
            next = rng.gen_range_usize(0, phi.len());
            attempts += 1;
        }
        if seeds.contains(&next) {
            if let Some(fallback) = (0..phi.len()).find(|&i| !seeds.contains(&i)) {
                next = fallback;
            } else {
                break;
            }
        }
        if !seeds.contains(&next) {
            seeds.push(next);
        } else {
            break;
        }
    }

    seeds
}

fn estimate_seed_min_spacing(plate_count: usize) -> f32 {
    let n = plate_count.max(2) as f32;
    let target_angle = (4.0 * std::f32::consts::PI / n).sqrt() * 0.55;
    clamp((2.0 - 2.0 * target_angle.cos()).max(0.0).sqrt(), 0.10, 0.75)
}

fn is_seed_far_enough(
    positions: &[[f32; 3]],
    seeds: &[usize],
    candidate: usize,
    min_spacing: f32,
) -> bool {
    for &s in seeds {
        if chord_distance(positions[candidate], positions[s]) < min_spacing {
            return false;
        }
    }
    true
}

fn take_spaced_candidates(
    seeds: &mut Vec<usize>,
    candidates: &[usize],
    positions: &[[f32; 3]],
    target_len: usize,
    min_spacing: f32,
) {
    if seeds.len() >= target_len {
        return;
    }
    for &candidate in candidates {
        if seeds.len() >= target_len {
            break;
        }
        if seeds.contains(&candidate) {
            continue;
        }
        if is_seed_far_enough(positions, seeds, candidate, min_spacing) {
            seeds.push(candidate);
        }
    }
}

fn farthest_point_seed(
    positions: &[[f32; 3]],
    seeds: &[usize],
    rng: &mut DeterministicRng,
) -> usize {
    if seeds.is_empty() {
        return rng.gen_range_usize(0, positions.len());
    }

    let mut best_idx = 0;
    let mut best_score = -1.0;

    for (i, p) in positions.iter().enumerate() {
        if seeds.contains(&i) {
            continue;
        }
        let mut min_dist = f32::MAX;
        for &s in seeds {
            let d = chord_distance(*p, positions[s]);
            if d < min_dist {
                min_dist = d;
            }
        }
        if min_dist > best_score {
            best_score = min_dist;
            best_idx = i;
        }
    }

    best_idx
}

fn build_plate_growth_profiles(plate_count: usize, rng: &mut DeterministicRng) -> Vec<PlateGrowthProfile> {
    let mut profiles = Vec::with_capacity(plate_count);
    for _ in 0..plate_count {
        let mut warp_weights = [
            rng.gen_range_f32(-1.0, 1.0),
            rng.gen_range_f32(-1.0, 1.0),
            rng.gen_range_f32(-1.0, 1.0),
        ];
        let norm = (warp_weights[0] * warp_weights[0]
            + warp_weights[1] * warp_weights[1]
            + warp_weights[2] * warp_weights[2])
            .sqrt()
            .max(1e-6);
        for w in &mut warp_weights {
            *w /= norm;
        }
        profiles.push(PlateGrowthProfile {
            spread: rng.gen_range_f32(0.65, 1.45),
            preferred_axis: random_unit_vector3(rng),
            secondary_axis: random_unit_vector3(rng),
            axis_blend_axis: random_unit_vector3(rng),
            anisotropy: rng.gen_range_f32(0.25, 0.95),
            roughness: rng.gen_range_f32(0.10, 0.30),
            warp_weights,
            warp_gain: rng.gen_range_f32(0.18, 0.42),
        });
    }
    profiles
}

fn edge_noise_signed(a: usize, b: usize, plate: usize) -> f32 {
    let (lo, hi) = if a <= b { (a as u64, b as u64) } else { (b as u64, a as u64) };
    let mut x = lo
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(hi.wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add((plate as u64).wrapping_mul(0x94D0_49BB_1331_11EB));
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    let u = ((x >> 40) as u32) as f32 / 16_777_215.0;
    2.0 * u - 1.0
}

fn generate_plate_cost_warp_basis(
    count: usize,
    nbr_offsets: &[u32],
    nbrs: &[u32],
    rng: &mut DeterministicRng,
) -> [Vec<f32>; 3] {
    let mut a = generate_smoothed_noise_band(count, nbr_offsets, nbrs, 6, 15, rng);
    let mut b = generate_smoothed_noise_band(count, nbr_offsets, nbrs, 2, 6, rng);
    let mut c = generate_smoothed_noise_band(count, nbr_offsets, nbrs, 1, 3, rng);
    normalize_zscore_if_var(&mut a);
    normalize_zscore_if_var(&mut b);
    normalize_zscore_if_var(&mut c);
    smooth_scalar_field(nbr_offsets, nbrs, &mut a, 1);
    smooth_scalar_field(nbr_offsets, nbrs, &mut b, 1);
    smooth_scalar_field(nbr_offsets, nbrs, &mut c, 1);
    normalize_zscore_if_var(&mut a);
    normalize_zscore_if_var(&mut b);
    normalize_zscore_if_var(&mut c);
    for i in 0..count {
        b[i] *= 1.30;
        c[i] *= 1.15;
    }
    [a, b, c]
}

fn sample_plate_warp_mid(
    profile: &PlateGrowthProfile,
    basis: &[Vec<f32>; 3],
    v0: usize,
    v1: usize,
) -> f32 {
    let mut acc = 0.0;
    for i in 0..3 {
        let mid = 0.5 * (basis[i][v0] + basis[i][v1]);
        acc += profile.warp_weights[i] * mid;
    }
    acc
}

fn local_preferred_tangent_axis(
    profile: &PlateGrowthProfile,
    position: [f32; 3],
    edge_dir: [f32; 3],
) -> [f32; 3] {
    let blend = 0.5 + 0.5 * clamp(dot3(position, profile.axis_blend_axis), -1.0, 1.0);
    let mixed = normalize3(add3(
        mul3(profile.preferred_axis, 1.0 - blend),
        mul3(profile.secondary_axis, blend),
    ));
    let tangent = normalize3(project_to_tangent(mixed, position));
    if length3(tangent) <= 1e-6 {
        let fallback = normalize3(project_to_tangent(profile.preferred_axis, position));
        if length3(fallback) <= 1e-6 {
            edge_dir
        } else {
            fallback
        }
    } else {
        tangent
    }
}

fn random_unit_vector3(rng: &mut DeterministicRng) -> [f32; 3] {
    let v = [
        rng.standard_normal(),
        rng.standard_normal(),
        rng.standard_normal(),
    ];
    let n = normalize3(v);
    if length3(n) <= 1e-6 {
        [1.0, 0.0, 0.0]
    } else {
        n
    }
}

fn partition_plates(
    positions: &[[f32; 3]],
    phi: &[f32],
    plate_cost_warp_basis: &[Vec<f32>; 3],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    seeds: &[usize],
    growth_profiles: &[PlateGrowthProfile],
    boundary_band: f32,
) -> Vec<u32> {
    let mut best_cost = vec![f32::INFINITY; positions.len()];
    let mut plate_id = vec![u32::MAX; positions.len()];
    let mut heap = BinaryHeap::<QueueState>::new();

    for (plate, &seed) in seeds.iter().enumerate() {
        best_cost[seed] = 0.0;
        plate_id[seed] = plate as u32;
        heap.push(QueueState {
            cost: 0.0,
            vertex: seed,
            plate,
        });
    }

    while let Some(state) = heap.pop() {
        if state.cost > best_cost[state.vertex] {
            continue;
        }
        let start = nbr_offsets[state.vertex] as usize;
        let end = nbr_offsets[state.vertex + 1] as usize;

        for &n in &nbrs[start..end] {
            let n = n as usize;
            let edge_len = chord_distance(positions[state.vertex], positions[n]);
            let phi_mid = 0.5 * (phi[state.vertex] + phi[n]);
            let penalty = clamp(phi_mid.abs() / boundary_band, 0.0, 1.0);
            let profile = &growth_profiles[state.plate];
            let spread = profile.spread.max(0.35);
            let edge_dir = normalize3(sub3(positions[n], positions[state.vertex]));
            let tangent_axis =
                local_preferred_tangent_axis(profile, positions[state.vertex], edge_dir);
            let alignment = dot3(edge_dir, tangent_axis).abs();
            let directional_factor = 1.0 + profile.anisotropy * (1.0 - clamp(alignment, 0.0, 1.0));
            let phi_discount = clamp(1.0 - 0.18 * phi_mid, 0.68, 1.30);
            let warp_mid = sample_plate_warp_mid(profile, plate_cost_warp_basis, state.vertex, n);
            let warp_factor = clamp(1.0 + profile.warp_gain * warp_mid, 0.62, 1.45);
            let random_factor =
                1.0 + profile.roughness * edge_noise_signed(state.vertex, n, state.plate);
            let next_cost = state.cost
                + edge_len
                    * (1.0 + penalty)
                    * directional_factor
                    * phi_discount
                    * warp_factor
                    * random_factor
                    / spread;

            if next_cost + 1e-7 < best_cost[n] {
                best_cost[n] = next_cost;
                plate_id[n] = state.plate as u32;
                heap.push(QueueState {
                    cost: next_cost,
                    vertex: n,
                    plate: state.plate,
                });
            }
        }
    }

    for v in 0..plate_id.len() {
        if plate_id[v] == u32::MAX {
            let mut best_seed = 0;
            let mut best_dist = f32::MAX;
            for (plate, &seed) in seeds.iter().enumerate() {
                let d = chord_distance(positions[v], positions[seed]);
                if d < best_dist {
                    best_dist = d;
                    best_seed = plate as u32;
                }
            }
            plate_id[v] = best_seed;
        }
    }

    plate_id
}

fn cleanup_plate_components(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &mut [u32],
    plate_count: usize,
) {
    if plate_id.is_empty() || plate_count == 0 {
        return;
    }
    let small_component_max = (plate_id.len() / (plate_count.max(1) * 18)).clamp(6, 64);

    for _ in 0..6 {
        let largest = largest_component_sizes_by_plate(nbr_offsets, nbrs, plate_id, plate_count);
        let mut visited = vec![false; plate_id.len()];
        let mut stack = Vec::<usize>::new();
        let mut relabel = Vec::<(usize, u32)>::new();
        let mut changed = false;

        for start_v in 0..plate_id.len() {
            if visited[start_v] {
                continue;
            }
            let plate = plate_id[start_v];
            if (plate as usize) >= plate_count {
                visited[start_v] = true;
                continue;
            }

            let mut component = Vec::<usize>::new();
            stack.push(start_v);
            visited[start_v] = true;

            while let Some(v) = stack.pop() {
                component.push(v);
                let start = nbr_offsets[v] as usize;
                let end = nbr_offsets[v + 1] as usize;
                for &n in &nbrs[start..end] {
                    let n = n as usize;
                    if visited[n] || plate_id[n] != plate {
                        continue;
                    }
                    visited[n] = true;
                    stack.push(n);
                }
            }

            let mut neighbor_counts = vec![0usize; plate_count];
            let mut unique_neighbors = 0usize;
            let mut best_neighbor = None::<usize>;
            let mut best_touch = 0usize;

            for &v in &component {
                let start = nbr_offsets[v] as usize;
                let end = nbr_offsets[v + 1] as usize;
                for &n in &nbrs[start..end] {
                    let n = n as usize;
                    let other = plate_id[n] as usize;
                    if other >= plate_count || other == plate as usize {
                        continue;
                    }
                    if neighbor_counts[other] == 0 {
                        unique_neighbors += 1;
                    }
                    neighbor_counts[other] += 1;
                    if neighbor_counts[other] > best_touch {
                        best_touch = neighbor_counts[other];
                        best_neighbor = Some(other);
                    }
                }
            }

            let is_enclave = unique_neighbors == 1 && best_neighbor.is_some();
            let is_small_fragment = component.len() <= small_component_max
                && component.len() < largest[plate as usize];

            if !(is_enclave || is_small_fragment) {
                continue;
            }

            let target = match best_neighbor {
                Some(v) => v as u32,
                None => continue,
            };
            for &v in &component {
                relabel.push((v, target));
            }
        }

        for (v, new_plate) in relabel {
            if plate_id[v] != new_plate {
                plate_id[v] = new_plate;
                changed = true;
            }
        }

        if !changed {
            break;
        }
    }
}

fn largest_component_sizes_by_plate(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    plate_count: usize,
) -> Vec<usize> {
    let mut largest = vec![0usize; plate_count];
    let mut visited = vec![false; plate_id.len()];
    let mut stack = Vec::<usize>::new();

    for start_v in 0..plate_id.len() {
        if visited[start_v] {
            continue;
        }
        visited[start_v] = true;
        let plate = plate_id[start_v] as usize;
        if plate >= plate_count {
            continue;
        }

        let mut size = 0usize;
        stack.push(start_v);
        while let Some(v) = stack.pop() {
            size += 1;
            let start = nbr_offsets[v] as usize;
            let end = nbr_offsets[v + 1] as usize;
            for &n in &nbrs[start..end] {
                let n = n as usize;
                if visited[n] || plate_id[n] as usize != plate {
                    continue;
                }
                visited[n] = true;
                stack.push(n);
            }
        }

        if size > largest[plate] {
            largest[plate] = size;
        }
    }

    largest
}

fn compact_plate_ids(mut plate_id: Vec<u32>, plate_count: usize) -> Vec<u32> {
    let mut counts = vec![0usize; plate_count];
    for &id in &plate_id {
        if (id as usize) < counts.len() {
            counts[id as usize] += 1;
        }
    }

    let fallback = counts
        .iter()
        .enumerate()
        .max_by_key(|(_, c)| **c)
        .map(|(i, _)| i as u32)
        .unwrap_or(0);

    for id in &mut plate_id {
        if (*id as usize) >= plate_count || counts[*id as usize] == 0 {
            *id = fallback;
        }
    }

    plate_id
}

fn assign_plate_attributes(
    plate_count: usize,
    rng: &mut DeterministicRng,
    ocean_plate_ratio: f32,
) -> Vec<PlateAttr> {
    let mut attrs = Vec::with_capacity(plate_count);

    for _ in 0..plate_count {
        let is_ocean = rng.bernoulli(ocean_plate_ratio);
        let dir = rng.gen_range_f32(0.0, 2.0 * std::f32::consts::PI);
        let speed = rng.gen_range_f32(0.3, 1.0);
        let velocity = [speed * dir.cos(), speed * dir.sin(), 0.0];

        let base_height = if is_ocean {
            -0.04 + rng.gen_range_f32(-0.03, 0.03)
        } else {
            0.10 + rng.gen_range_f32(-0.08, 0.08)
        };
        let base_weight = if is_ocean {
            0.62 + rng.gen_range_f32(-0.06, 0.08)
        } else {
            0.22 + rng.gen_range_f32(-0.04, 0.04)
        };

        attrs.push(PlateAttr {
            is_ocean,
            velocity,
            base_height,
            base_weight,
        });
    }

    attrs
}

fn compute_vertex_lithosphere(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    attributes: &[PlateAttr],
    boundary_edges: &[BoundaryEdge],
) -> Vec<VertexLithosphere> {
    let v_count = positions.len();
    let mut crust_age_dist = vec![f32::INFINITY; v_count];
    let mut lith = vec![
        VertexLithosphere {
            age_norm: 0.0,
            weight: 0.0,
            buoyancy: 0.0,
        };
        v_count
    ];
    let mut heap = BinaryHeap::new();
    let plate_count = attributes.len();

    let mut has_divergent_source = vec![false; plate_count];
    let mut has_boundary_seed = vec![false; plate_count];

    for i in 0..v_count {
        let pid = plate_id[i] as usize;
        lith[i].weight = attributes[pid].base_weight;
        lith[i].buoyancy = attributes[pid].base_height;
    }

    for edge in boundary_edges {
        let is_divergent = matches!(edge.boundary_type, BoundaryType::Divergent);
        for &v in &[edge.a, edge.b] {
            let pv = plate_id[v] as usize;
            if !attributes[pv].is_ocean {
                continue;
            }
            has_boundary_seed[pv] = true;
            if is_divergent {
                has_divergent_source[pv] = true;
                if crust_age_dist[v] > 0.0 {
                    crust_age_dist[v] = 0.0;
                    heap.push(BoundaryDistState {
                        cost: 0.0,
                        vertex: v,
                        source_edge: v,
                    });
                }
            }
        }
    }

    for i in 0..v_count {
        let pid = plate_id[i] as usize;
        if !attributes[pid].is_ocean {
            continue;
        }
        if has_divergent_source[pid] {
            continue;
        }
        if has_boundary_seed[pid] && crust_age_dist[i].is_infinite() {
            let start = nbr_offsets[i] as usize;
            let end = nbr_offsets[i + 1] as usize;
            let is_boundary = nbrs[start..end]
                .iter()
                .any(|&n| plate_id[n as usize] != plate_id[i]);
            if is_boundary {
                crust_age_dist[i] = 0.0;
                heap.push(BoundaryDistState {
                    cost: 0.0,
                    vertex: i,
                    source_edge: i,
                });
            }
        }
    }

    while let Some(state) = heap.pop() {
        if state.cost > crust_age_dist[state.vertex] + 1e-6 {
            continue;
        }
        let pid = plate_id[state.vertex] as usize;
        if !attributes[pid].is_ocean {
            continue;
        }

        let start = nbr_offsets[state.vertex] as usize;
        let end = nbr_offsets[state.vertex + 1] as usize;
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if plate_id[n] != plate_id[state.vertex] {
                continue;
            }
            let npid = plate_id[n] as usize;
            if !attributes[npid].is_ocean {
                continue;
            }
            let step = chord_distance(positions[state.vertex], positions[n]).max(1e-4);
            let next_cost = state.cost + step;
            if next_cost + 1e-6 < crust_age_dist[n] {
                crust_age_dist[n] = next_cost;
                heap.push(BoundaryDistState {
                    cost: next_cost,
                    vertex: n,
                    source_edge: state.source_edge,
                });
            }
        }
    }

    let mut ocean_plate_max_age = vec![0.0_f32; plate_count];
    for v in 0..v_count {
        let pid = plate_id[v] as usize;
        if !attributes[pid].is_ocean {
            continue;
        }
        if crust_age_dist[v].is_finite() {
            ocean_plate_max_age[pid] = ocean_plate_max_age[pid].max(crust_age_dist[v]);
        }
    }

    for v in 0..v_count {
        let pid = plate_id[v] as usize;
        if !attributes[pid].is_ocean {
            lith[v].age_norm = 0.0;
            lith[v].weight = attributes[pid].base_weight;
            lith[v].buoyancy = attributes[pid].base_height;
            continue;
        }
        let max_age = ocean_plate_max_age[pid].max(1e-4);
        let age = if crust_age_dist[v].is_finite() {
            clamp(crust_age_dist[v] / max_age, 0.0, 1.0)
        } else {
            0.0
        };
        let weight = attributes[pid].base_weight + 0.42 * age;
        // 海洋の標高は「海嶺で軽く高い → 老化で重く低い」を浮力で一元表現する。
        let buoyancy = (-0.08 + 0.06 * (1.0 - age)) - 0.26 * (weight - 0.62);
        lith[v] = VertexLithosphere {
            age_norm: age,
            weight,
            buoyancy,
        };
    }

    lith
}

fn apply_boundary_model(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    attributes: &[PlateAttr],
    vertex_lithosphere: &[VertexLithosphere],
    boundary_edges: &[BoundaryEdge],
    height: &mut [f32],
    params: &TerrainParams,
) -> BoundaryFields {
    if boundary_edges.is_empty() {
        return BoundaryFields {
            preserve_strength: vec![0.0; height.len()],
        };
    }

    let (nearest_edge, boundary_dist, boundary_vertices) =
        compute_boundary_distance_assignment(positions, nbr_offsets, nbrs, &boundary_edges, height.len());

    let mut delta = vec![0.0_f32; height.len()];
    let mut preserve_strength = vec![0.0_f32; height.len()];

    for v in 0..height.len() {
        let edge_idx = nearest_edge[v];
        if edge_idx == usize::MAX {
            continue;
        }
        let edge = boundary_edges[edge_idx];
        let pid = plate_id[v] as usize;
        let d = boundary_dist[v];
        let dist_scale = (-(d * params.boundary_distance_falloff)).exp();

        match edge.boundary_type {
            BoundaryType::Convergent => {
                let oblique_relief = 1.0 - params.boundary_obliquity_mix * edge.obliquity;
                let conv_base = params.boundary_convergent_base_gain * edge.strength * oblique_relief;

                let (convergent_mode, subduction_polarity) = classify_convergent_edge(
                    edge.a,
                    edge.b,
                    edge.plate_a,
                    edge.plate_b,
                    attributes,
                    vertex_lithosphere,
                );
                if let Some(mode) = convergent_mode {
                    match mode {
                        ConvergentMode::ContinentContinent => {
                            let w = band_weight(d, params.boundary_width_collision, params.boundary_anisotropy);
                            let uplift = conv_base * params.collision_gain * w;
                            delta[v] += uplift;
                            if d < params.boundary_width_collision * 0.55 {
                                delta[v] -= 0.10 * uplift;
                            }
                            preserve_strength[v] = preserve_strength[v].max(0.80 * w);
                        }
                        ConvergentMode::OceanContinent | ConvergentMode::OceanOcean => {
                            let (subducting, overriding) = match subduction_polarity {
                                SubductionPolarity::AUnderB => (edge.plate_a, edge.plate_b),
                                SubductionPolarity::BUnderA => (edge.plate_b, edge.plate_a),
                                SubductionPolarity::None => (usize::MAX, usize::MAX),
                            };

                            if pid == subducting {
                                let trench_w = band_weight(
                                    d,
                                    params.boundary_width_trench * (0.9 + 0.35 * edge.obliquity),
                                    params.boundary_anisotropy,
                                );
                                let trench = conv_base * params.trench_gain * trench_w;
                                delta[v] -= trench;
                                let outer_rise = ring_weight(
                                    d,
                                    params.boundary_width_trench * 1.6,
                                    params.boundary_width_trench * 0.65,
                                );
                                delta[v] += 0.12 * conv_base * outer_rise * dist_scale;
                                preserve_strength[v] = preserve_strength[v].max(0.95 * trench_w);
                            } else if pid == overriding {
                                let forearc_w = band_weight(
                                    d,
                                    params.boundary_width_trench * 1.35,
                                    params.boundary_anisotropy * 0.6,
                                );
                                delta[v] -= 0.08 * conv_base * forearc_w;

                                let arc_center =
                                    params.boundary_width_arc * (0.9 + 0.4 * edge.obliquity);
                                let arc_w = ring_weight(
                                    d,
                                    arc_center,
                                    params.boundary_width_arc * 0.55,
                                );
                                let arc_gain = if matches!(mode, ConvergentMode::OceanOcean) {
                                    params.arc_gain * 1.15
                                } else {
                                    params.arc_gain
                                };
                                delta[v] += conv_base * arc_gain * arc_w * dist_scale;
                                preserve_strength[v] =
                                    preserve_strength[v].max(0.85 * forearc_w.max(arc_w));
                            }
                        }
                    }
                }
            }
            BoundaryType::Divergent => {
                let mut rift_width = params.boundary_width_rift;
                if !attributes[edge.plate_a].is_ocean && !attributes[edge.plate_b].is_ocean {
                    rift_width *= 1.35;
                }
                let oblique_relief = 1.0 - 0.6 * params.boundary_obliquity_mix * edge.obliquity;
                let rift_w = band_weight(d, rift_width, params.boundary_anisotropy * 0.8);
                let rift = params.boundary_divergent_base_gain
                    * params.rift_gain
                    * edge.strength
                    * oblique_relief
                    * rift_w;
                delta[v] -= rift;
                if d < rift_width * 0.65 {
                    delta[v] -=
                        0.05 * params.boundary_divergent_base_gain * edge.strength * rift_w;
                }
                preserve_strength[v] = preserve_strength[v].max(0.55 * rift_w);
            }
            BoundaryType::Transform => {
                let width = params.boundary_width_trench * 0.9;
                let w = band_weight(d, width, params.boundary_anisotropy * 0.5);
                let sign = if ((v as u32).wrapping_mul(1103515245) ^ (edge_idx as u32)) & 1 == 0 {
                    1.0
                } else {
                    -1.0
                };
                let relief = params.boundary_transform_relief_gain
                    * edge.strength
                    * (1.0 + 0.4 * edge.obliquity)
                    * w;
                delta[v] += sign * 0.5 * relief;
                preserve_strength[v] = preserve_strength[v].max(0.35 * w);
            }
        }
    }

    for v in 0..height.len() {
        let boosted = if boundary_vertices.mask[v] { delta[v] * 1.20 } else { delta[v] };
        height[v] = clamp(height[v] + boosted, -1.0, 1.0);
    }

    BoundaryFields { preserve_strength }
}

fn extract_boundary_edges(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    attributes: &[PlateAttr],
) -> Vec<BoundaryEdge> {
    let mut edges = Vec::new();
    let classify_eps = 0.05;

    for i in 0..positions.len() {
        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        for &j_u32 in &nbrs[start..end] {
            let j = j_u32 as usize;
            if j <= i {
                continue;
            }

            let plate_a = plate_id[i] as usize;
            let plate_b = plate_id[j] as usize;
            if plate_a == plate_b {
                continue;
            }

            let edge_vec = sub3(positions[j], positions[i]);
            let edge_dir = normalize3(edge_vec);
            let rel_v = sub3(attributes[plate_b].velocity, attributes[plate_a].velocity);
            let v_rel_n = dot3(rel_v, edge_dir);
            let v_rel_t_vec = sub3(rel_v, mul3(edge_dir, v_rel_n));
            let v_rel_t = length3(v_rel_t_vec);
            let obliquity = v_rel_t / (v_rel_t + v_rel_n.abs() + 1e-5);
            let (boundary_type, strength) = if v_rel_n > classify_eps {
                (
                    BoundaryType::Convergent,
                    clamp((v_rel_n - classify_eps) / 0.25, 0.0, 1.0),
                )
            } else if v_rel_n < -classify_eps {
                (
                    BoundaryType::Divergent,
                    clamp((-v_rel_n - classify_eps) / 0.25, 0.0, 1.0),
                )
            } else {
                (
                    BoundaryType::Transform,
                    clamp((v_rel_t - 0.02) / 0.18, 0.0, 1.0),
                )
            };

            edges.push(BoundaryEdge {
                a: i,
                b: j,
                plate_a,
                plate_b,
                boundary_type,
                strength: strength.max(0.05),
                obliquity,
            });
        }
    }

    edges
}

fn classify_convergent_edge(
    vertex_a: usize,
    vertex_b: usize,
    plate_a: usize,
    plate_b: usize,
    attributes: &[PlateAttr],
    vertex_lithosphere: &[VertexLithosphere],
) -> (Option<ConvergentMode>, SubductionPolarity) {
    let a_ocean = attributes[plate_a].is_ocean;
    let b_ocean = attributes[plate_b].is_ocean;

    if a_ocean && !b_ocean {
        return (Some(ConvergentMode::OceanContinent), SubductionPolarity::AUnderB);
    }
    if !a_ocean && b_ocean {
        return (Some(ConvergentMode::OceanContinent), SubductionPolarity::BUnderA);
    }
    if a_ocean && b_ocean {
        let a_weight = vertex_lithosphere[vertex_a].weight;
        let b_weight = vertex_lithosphere[vertex_b].weight;
        let polarity = if a_weight >= b_weight {
            SubductionPolarity::AUnderB
        } else {
            SubductionPolarity::BUnderA
        };
        return (Some(ConvergentMode::OceanOcean), polarity);
    }

    (Some(ConvergentMode::ContinentContinent), SubductionPolarity::None)
}

fn compute_boundary_distance_assignment(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    boundary_edges: &[BoundaryEdge],
    vertex_count: usize,
) -> (Vec<usize>, Vec<f32>, BoundaryVertices) {
    let mut nearest_edge = vec![usize::MAX; vertex_count];
    let mut dist = vec![f32::INFINITY; vertex_count];
    let mut boundary_vertices = BoundaryVertices::new(vertex_count);
    let mut heap = BinaryHeap::new();

    for (edge_idx, edge) in boundary_edges.iter().enumerate() {
        for &v in &[edge.a, edge.b] {
            boundary_vertices.insert(v);
            if 0.0 < dist[v] {
                dist[v] = 0.0;
                nearest_edge[v] = edge_idx;
                heap.push(BoundaryDistState {
                    cost: 0.0,
                    vertex: v,
                    source_edge: edge_idx,
                });
            }
        }
    }

    while let Some(state) = heap.pop() {
        if state.cost > dist[state.vertex] + 1e-6 {
            continue;
        }

        let start = nbr_offsets[state.vertex] as usize;
        let end = nbr_offsets[state.vertex + 1] as usize;
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            let step = chord_distance(positions[state.vertex], positions[n]).max(1e-4);
            let next_cost = state.cost + step;
            if next_cost + 1e-6 < dist[n] {
                dist[n] = next_cost;
                nearest_edge[n] = state.source_edge;
                heap.push(BoundaryDistState {
                    cost: next_cost,
                    vertex: n,
                    source_edge: state.source_edge,
                });
            }
        }
    }

    (nearest_edge, dist, boundary_vertices)
}

fn band_weight(distance: f32, width: f32, anisotropy: f32) -> f32 {
    let sigma = (width * (1.0 - 0.35 * anisotropy)).max(1e-4);
    (-(distance * distance) / (2.0 * sigma * sigma)).exp()
}

fn ring_weight(distance: f32, center: f32, width: f32) -> f32 {
    let sigma = width.max(1e-4);
    let dx = distance - center;
    (-(dx * dx) / (2.0 * sigma * sigma)).exp()
}

fn smooth_heights(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    boundary_fields: &BoundaryFields,
    height: &mut [f32],
    smooth_iter: u32,
    smooth_lambda: f32,
) {
    let mut buffer = height.to_vec();

    for _ in 0..smooth_iter {
        for v in 0..height.len() {
            let start = nbr_offsets[v] as usize;
            let end = nbr_offsets[v + 1] as usize;
            let degree = (end - start) as f32;
            if degree == 0.0 {
                buffer[v] = height[v];
                continue;
            }

            let mut sum = 0.0;
            for &n in &nbrs[start..end] {
                sum += height[n as usize];
            }
            let mean = sum / degree;

            let h = height[v];
            let is_coast = nbrs[start..end].iter().any(|&n| {
                let nh = height[n as usize];
                (h > 0.0 && nh <= 0.0) || (h <= 0.0 && nh > 0.0)
            });
            let preserve = boundary_fields.preserve_strength[v];
            let boundary_scale = lerp(1.0, 0.45, preserve);
            let terrain_scale = if h > 0.35 {
                0.70
            } else if h > 0.0 {
                0.90
            } else if h < -0.35 {
                1.15
            } else {
                1.00
            };
            let coast_scale = if is_coast { 0.82 } else { 1.0 };
            let lambda = clamp(
                smooth_lambda * boundary_scale * terrain_scale * coast_scale,
                0.0,
                1.0,
            );
            buffer[v] = clamp(h + lambda * (mean - h), -1.0, 1.0);
        }
        height.copy_from_slice(&buffer);
    }
}

fn postprocess_height(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &mut [f32],
    target_sea_ratio: f32,
) {
    let mut sorted = height.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let sea_idx = ((sorted.len() as f32) * target_sea_ratio) as usize;
    let sea_idx = sea_idx.min(sorted.len().saturating_sub(1));
    let sea_level = sorted[sea_idx];

    for h in height.iter_mut() {
        *h -= sea_level;
        *h *= 0.58;
        *h = clamp(*h, -1.0, 1.0);
    }

    let mut coast = vec![false; height.len()];
    for v in 0..height.len() {
        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;
        for &n in &nbrs[start..end] {
            let n = n as usize;
            if (height[v] > 0.0 && height[n] <= 0.0) || (height[v] <= 0.0 && height[n] > 0.0) {
                coast[v] = true;
                break;
            }
        }
    }

    for v in 0..height.len() {
        if coast[v] && height[v] > 0.0 {
            height[v] *= 0.62;
        }
        if height[v] > 0.0 && height[v] < 0.15 {
            height[v] *= 0.78;
        }
        if height[v] < 0.0 && height[v] > -0.10 {
            height[v] *= 0.80;
        }
        height[v] = clamp(height[v], -1.0, 1.0);
    }
}

fn apply_hydraulic_erosion(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &mut [f32],
    params: &TerrainParams,
) {
    if params.erosion_iter == 0
        || params.hydraulic_erode_rate <= 0.0
        || params.erosion_max_delta_per_iter <= 0.0
    {
        return;
    }

    let v_count = height.len();
    let mut next_height = height.to_vec();
    let mut delta = vec![0.0; v_count];

    for _ in 0..params.erosion_iter {
        let (river_flux, river_next) = compute_river_flux_and_next(
            positions,
            nbr_offsets,
            nbrs,
            height,
            params.river_rain_base,
        );

        delta.fill(0.0);

        for i in 0..v_count {
            if height[i] <= 0.0 {
                continue;
            }

            let next = river_next[i];
            if next < 0 {
                continue;
            }
            let n = next as usize;

            let edge_len = chord_distance(positions[i], positions[n]).max(1e-4);
            let raw_drop = (height[i] - height[n]).max(0.0);
            let local_slope = raw_drop / edge_len;
            let effective_slope = local_slope.max(params.erosion_min_slope);
            let capacity = params.sediment_capacity_gain * river_flux[i] * effective_slope;

            let erode_amount = clamp(
                params.hydraulic_erode_rate * capacity,
                0.0,
                params.erosion_max_delta_per_iter,
            );
            if erode_amount <= 0.0 {
                continue;
            }

            delta[i] -= erode_amount;

            let deposit_amount = if height[n] > 0.0 {
                let downstream_slope = if river_next[n] >= 0 {
                    let nn = river_next[n] as usize;
                    let next_len = chord_distance(positions[n], positions[nn]).max(1e-4);
                    ((height[n] - height[nn]).max(0.0)) / next_len
                } else {
                    0.0
                };
                let flattening = clamp(
                    1.0 - downstream_slope / (local_slope.max(params.erosion_min_slope) + 1e-6),
                    0.0,
                    1.0,
                );
                erode_amount * params.hydraulic_deposit_rate * flattening
            } else if height[n] > params.shallow_sea_floor {
                erode_amount * params.hydraulic_deposit_rate * params.coastal_deposit_rate
            } else {
                0.0
            };

            if deposit_amount > 0.0 {
                delta[n] += deposit_amount;
            }
        }

        for i in 0..v_count {
            next_height[i] = clamp(height[i] + delta[i], -1.2, 1.2);
        }
        height.copy_from_slice(&next_height);
    }
}

fn compute_river_flux_and_next(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    river_rain_base: f32,
) -> (Vec<f32>, Vec<i32>) {
    let v_count = positions.len();
    let rain = build_precipitation_map(positions, nbr_offsets, nbrs, height, river_rain_base);
    let mut river_next = vec![-1; v_count];
    let mut river_flux = vec![0.0; v_count];

    for i in 0..v_count {
        if height[i] <= 0.0 {
            continue;
        }

        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;

        let mut best = -1;
        let mut best_drop = 0.0;

        for &n in &nbrs[start..end] {
            let n = n as usize;
            let drop = height[i] - height[n];
            if drop > best_drop {
                best_drop = drop;
                best = n as i32;
            }
        }

        river_next[i] = if best_drop > 0.0 { best } else { -1 };
    }

    let mut order = (0..v_count).collect::<Vec<_>>();
    order.sort_by(|a, b| {
        height[*b]
            .partial_cmp(&height[*a])
            .unwrap_or(Ordering::Equal)
    });

    for &i in &order {
        river_flux[i] += rain[i];
        let next = river_next[i];
        if next >= 0 {
            river_flux[next as usize] += river_flux[i];
        }
    }

    let max_flux = river_flux
        .iter()
        .copied()
        .fold(0.0_f32, |acc, v| if v > acc { v } else { acc })
        .max(1e-6);

    for value in &mut river_flux {
        *value /= max_flux;
    }

    (river_flux, river_next)
}

fn generate_rivers(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    river_rain_base: f32,
    river_accum_threshold: f32,
) -> (Vec<f32>, Vec<i32>) {
    let (mut river_flux, mut river_next) =
        compute_river_flux_and_next(positions, nbr_offsets, nbrs, height, river_rain_base);

    for i in 0..positions.len() {
        if river_flux[i] < river_accum_threshold {
            river_flux[i] = 0.0;
        }
        if height[i] <= 0.0 {
            river_next[i] = -1;
        }
    }

    (river_flux, river_next)
}

fn build_precipitation_map(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    river_rain_base: f32,
) -> Vec<f32> {
    let mut rain = vec![0.0; positions.len()];

    for i in 0..positions.len() {
        let p = positions[i];
        let lat = clamp(p[1], -1.0, 1.0).asin();

        let lat_factor = (1.0 - lat.abs() / (std::f32::consts::PI * 0.5)).max(0.0);
        let altitude_factor = 1.0 + 0.20 * height[i].max(0.0);

        let wind_dir = prevailing_wind_dir(p, lat);
        let (upwind_h, downwind_h) =
            directional_neighbor_heights(i, positions, nbr_offsets, nbrs, height, wind_dir);

        let slope_signal = clamp((downwind_h - upwind_h) / 0.20, -1.0, 1.0);
        let windward_boost = slope_signal.max(0.0);
        let leeward_drop = (-slope_signal).max(0.0);
        let barrier_strength = upwind_h.max(0.0);

        let orographic_factor = clamp(
            1.0 + 0.60 * windward_boost * (1.0 + 0.6 * height[i].max(0.0))
                - 1.10 * leeward_drop * (1.0 + 0.8 * barrier_strength),
            0.12,
            2.20,
        );

        rain[i] = river_rain_base * lat_factor * altitude_factor * orographic_factor;
    }

    rain
}

fn prevailing_wind_dir(p: [f32; 3], lat: f32) -> [f32; 3] {
    let abs_lat = lat.abs();
    let zonal_sign = if abs_lat < std::f32::consts::FRAC_PI_6 {
        -1.0
    } else if abs_lat < std::f32::consts::PI / 3.0 {
        1.0
    } else {
        -1.0
    };

    let mut east = [-p[2], 0.0, p[0]];
    if length3(east) < 1e-6 {
        east = project_to_tangent([1.0, 0.0, 0.0], p);
    }
    east = normalize3(east);

    let pole = if lat >= 0.0 {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, -1.0, 0.0]
    };
    let meridional = normalize3(project_to_tangent(pole, p));
    let meridional_sign = if abs_lat < std::f32::consts::FRAC_PI_6 {
        -1.0
    } else {
        0.35
    };

    normalize3(add3(
        mul3(east, zonal_sign),
        mul3(meridional, 0.25 * meridional_sign),
    ))
}

fn directional_neighbor_heights(
    i: usize,
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    wind_dir: [f32; 3],
) -> (f32, f32) {
    let p = positions[i];
    let start = nbr_offsets[i] as usize;
    let end = nbr_offsets[i + 1] as usize;

    let mut up_sum = 0.0;
    let mut up_w = 0.0;
    let mut down_sum = 0.0;
    let mut down_w = 0.0;

    for &n in &nbrs[start..end] {
        let n = n as usize;
        let edge = sub3(positions[n], p);
        let tangent = project_to_tangent(edge, p);
        let len = length3(tangent);
        if len < 1e-6 {
            continue;
        }
        let dir = [tangent[0] / len, tangent[1] / len, tangent[2] / len];
        let score = dot3(dir, wind_dir);

        if score > 0.15 {
            let w = score * score;
            down_sum += height[n] * w;
            down_w += w;
        } else if score < -0.15 {
            let w = score * score;
            up_sum += height[n] * w;
            up_w += w;
        }
    }

    let upwind_h = if up_w > 0.0 { up_sum / up_w } else { height[i] };
    let downwind_h = if down_w > 0.0 {
        down_sum / down_w
    } else {
        height[i]
    };
    (upwind_h, downwind_h)
}

fn earth_preset(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    river_rain_base: f32,
) -> TerrainOutput {
    let mut height = vec![0.0; positions.len()];
    let mut plate_id = vec![0u32; positions.len()];

    let continents = [
        ([0.90, 0.15, 0.35], 0.55, 0.75),
        ([-0.70, 0.35, 0.10], 0.48, 0.70),
        ([0.10, -0.20, -0.95], 0.52, 0.72),
        ([-0.20, -0.65, 0.70], 0.42, 0.68),
    ];

    for (i, p) in positions.iter().enumerate() {
        let lat = clamp(p[1], -1.0, 1.0).asin();
        let mut land_signal = -0.35 + 0.10 * (2.0 * lat).cos();

        for (center, amp, width) in continents {
            let d = chord_distance(*p, center);
            let influence = amp * (-(d * d) / (2.0 * width * width)).exp();
            land_signal += influence;
        }

        let ridge = 0.08 * (6.0 * p[0]).sin() * (5.0 * p[2]).cos();
        height[i] = clamp(land_signal + ridge, -1.0, 1.0);

        plate_id[i] = if height[i] > 0.35 {
            1
        } else if height[i] > 0.05 {
            2
        } else if p[0] > 0.0 {
            0
        } else {
            3
        };
    }

    let (river_flux, river_next) = generate_rivers(
        positions,
        nbr_offsets,
        nbrs,
        &height,
        river_rain_base,
        0.015,
    );

    TerrainOutput {
        height,
        plate_id,
        river_flux,
        river_next,
        vertex_weight: vec![0.66, 0.24, 0.20, 0.61],
        plate_is_ocean: vec![1, 0, 0, 1],
        plate_base_height: vec![-0.06, 0.14, 0.08, -0.03],
        plate_base_weight: vec![0.66, 0.24, 0.20, 0.61],
    }
}

#[cfg(test)]
mod tests {
    use super::{generate_icosphere, generate_rivers, normalize_zscore, rng_from_seed};
    use crate::{TerrainOutput, TerrainParams};

    fn generate_for_test(seed: &str, params: &TerrainParams) -> TerrainOutput {
        let mut rng = rng_from_seed(seed, params);
        let (positions, indices) = generate_icosphere(params.level);
        let (nbr_offsets, nbrs) = super::build_neighbors(positions.len(), &indices);
        let spherical = super::compute_spherical_coords(&positions);

        let mut phi = super::evaluate_phi(&spherical, params.l_max, params.alpha, &mut rng);
        normalize_zscore(&mut phi);
        let plate_count =
            super::choose_plate_count(params.num_plates_min, params.num_plates_max, &mut rng);
        let seeds =
            super::pick_plate_seeds(&phi, &positions, &nbr_offsets, &nbrs, plate_count, &mut rng);
        let growth_profiles = super::build_plate_growth_profiles(plate_count, &mut rng);
        let plate_cost_warp_basis =
            super::generate_plate_cost_warp_basis(positions.len(), &nbr_offsets, &nbrs, &mut rng);
        let mut plate_id = super::partition_plates(
            &positions,
            &phi,
            &plate_cost_warp_basis,
            &nbr_offsets,
            &nbrs,
            &seeds,
            &growth_profiles,
            params.boundary_band,
        );
        plate_id = super::compact_plate_ids(plate_id, plate_count);
        super::cleanup_plate_components(&nbr_offsets, &nbrs, &mut plate_id, plate_count);
        plate_id = super::compact_plate_ids(plate_id, plate_count);

        let attributes =
            super::assign_plate_attributes(plate_count, &mut rng, params.ocean_plate_ratio);
        let boundary_edges =
            super::extract_boundary_edges(&positions, &nbr_offsets, &nbrs, &plate_id, &attributes);
        let vertex_lithosphere = super::compute_vertex_lithosphere(
            &positions,
            &nbr_offsets,
            &nbrs,
            &plate_id,
            &attributes,
            &boundary_edges,
        );
        let mut height = vec![0.0; positions.len()];
        for v in 0..positions.len() {
            let pid = plate_id[v] as usize;
            let noise = rng.gen_range_f32(-0.03, 0.03);
            let crust_base = if attributes[pid].is_ocean {
                vertex_lithosphere[v].buoyancy
            } else {
                attributes[pid].base_height
            };
            height[v] = super::clamp(
                crust_base + 0.10 * phi[v] + noise,
                -1.2,
                1.2,
            );
        }

        let boundary_fields = super::apply_boundary_model(
            &positions,
            &nbr_offsets,
            &nbrs,
            &plate_id,
            &attributes,
            &vertex_lithosphere,
            &boundary_edges,
            &mut height,
            params,
        );

        super::smooth_heights(
            &nbr_offsets,
            &nbrs,
            &boundary_fields,
            &mut height,
            params.smooth_iter,
            params.smooth_lambda,
        );
        super::apply_hydraulic_erosion(&positions, &nbr_offsets, &nbrs, &mut height, params);
        super::postprocess_height(
            &nbr_offsets,
            &nbrs,
            &mut height,
            super::clamp(params.ocean_plate_ratio + 0.04, 0.55, 0.78),
        );

        let (river_flux, river_next) = generate_rivers(
            &positions,
            &nbr_offsets,
            &nbrs,
            &height,
            params.river_rain_base,
            params.river_accum_threshold,
        );
        let vertex_weight = vertex_lithosphere
            .iter()
            .map(|lith| lith.weight)
            .collect::<Vec<_>>();
        let plate_is_ocean = attributes
            .iter()
            .map(|attr| u8::from(attr.is_ocean))
            .collect::<Vec<_>>();
        let plate_base_height = attributes
            .iter()
            .map(|attr| attr.base_height)
            .collect::<Vec<_>>();
        let plate_base_weight = attributes
            .iter()
            .map(|attr| attr.base_weight)
            .collect::<Vec<_>>();

        TerrainOutput {
            height,
            plate_id,
            river_flux,
            river_next,
            vertex_weight,
            plate_is_ocean,
            plate_base_height,
            plate_base_weight,
        }
    }

    #[test]
    fn level_zero_has_expected_topology() {
        let (positions, indices) = generate_icosphere(0);
        assert_eq!(positions.len(), 12);
        assert_eq!(indices.len(), 60);
    }

    #[test]
    fn level_six_has_expected_counts() {
        let (positions, indices) = generate_icosphere(6);
        let expected_faces = 20 * 4_u32.pow(6);
        let expected_vertices = 10 * 4_u32.pow(6) + 2;
        assert_eq!(indices.len() as u32, expected_faces * 3);
        assert_eq!(positions.len() as u32, expected_vertices);
    }

    #[test]
    fn terrain_output_has_consistent_lengths() {
        let params = TerrainParams {
            level: 3,
            ..TerrainParams::default()
        };
        let output = generate_for_test("alpha", &params);
        let v = output.height.len();
        assert_eq!(output.plate_id.len(), v);
        assert_eq!(output.river_flux.len(), v);
        assert_eq!(output.river_next.len(), v);
        assert!(output.height.iter().all(|h| *h >= -1.0 && *h <= 1.0));
    }

    #[test]
    fn terrain_generation_is_deterministic() {
        let params = TerrainParams {
            level: 3,
            ..TerrainParams::default()
        };

        let a = generate_for_test("same-seed", &params);
        let b = generate_for_test("same-seed", &params);

        assert_eq!(a.plate_id, b.plate_id);
        for (ha, hb) in a.height.iter().zip(b.height.iter()) {
            assert!((ha - hb).abs() <= 1e-6);
        }
    }

    #[test]
    fn hydraulic_erosion_is_noop_when_iter_zero() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = super::build_neighbors(positions.len(), &indices);
        let mut height = positions
            .iter()
            .map(|p| p[1] * 0.2 + 0.05)
            .collect::<Vec<_>>();
        let original = height.clone();

        let params = TerrainParams {
            erosion_iter: 0,
            ..TerrainParams::default()
        };

        super::apply_hydraulic_erosion(&positions, &nbr_offsets, &nbrs, &mut height, &params);
        assert_eq!(height, original);
    }
}
