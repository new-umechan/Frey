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
}

#[derive(Clone, Copy)]
enum BoundaryType {
    Convergent,
    Divergent,
    Transform,
}

#[derive(Clone, Copy)]
struct QueueState {
    cost: f32,
    vertex: usize,
    plate: usize,
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
    let spread_factors = build_plate_spread_factors(plate_count, &mut rng);
    let mut plate_id = partition_plates(
        &positions,
        &phi,
        &nbr_offsets,
        &nbrs,
        &seeds,
        &spread_factors,
        params.boundary_band,
    );
    plate_id = compact_plate_ids(plate_id, plate_count);
    let plate_boundary_proximity =
        compute_plate_boundary_proximity(&nbr_offsets, &nbrs, &plate_id, 3);

    let attributes = assign_plate_attributes(plate_count, &mut rng, params.ocean_plate_ratio);
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
        height[v] = clamp(
            attributes[pid].base_height
                + 0.08 * phi[v]
                + low_amp * band_low[v]
                + mid_amp * band_mid[v]
                + high_amp * band_high[v]
                + jitter,
            -1.2,
            1.2,
        );
    }

    let boundary_vertices = apply_boundary_interactions(
        &positions,
        &nbr_offsets,
        &nbrs,
        &plate_id,
        &attributes,
        &mut height,
        params.uplift_gain,
        params.subduct_gain,
        params.divergent_gain,
    );

    smooth_heights(
        &nbr_offsets,
        &nbrs,
        &boundary_vertices,
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

    let output = TerrainOutput {
        height,
        plate_id,
        river_flux,
        river_next,
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
    seeds.extend(max_candidates.iter().take(k_up).copied());
    seeds.extend(min_candidates.iter().take(k_down).copied());

    while seeds.len() < plate_count {
        let next = farthest_point_seed(positions, &seeds, rng);
        if !seeds.contains(&next) {
            seeds.push(next);
        } else {
            break;
        }
    }

    if seeds.is_empty() {
        seeds.push(rng.gen_range_usize(0, phi.len()));
    }

    while seeds.len() < plate_count {
        seeds.push(rng.gen_range_usize(0, phi.len()));
    }

    seeds
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

fn build_plate_spread_factors(plate_count: usize, rng: &mut DeterministicRng) -> Vec<f32> {
    let mut factors = Vec::with_capacity(plate_count);
    for _ in 0..plate_count {
        factors.push(rng.gen_range_f32(0.65, 1.45));
    }
    factors
}

fn partition_plates(
    positions: &[[f32; 3]],
    phi: &[f32],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    seeds: &[usize],
    spread_factors: &[f32],
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
            let spread = spread_factors[state.plate].max(0.35);
            let next_cost = state.cost + edge_len * (1.0 + penalty) / spread;

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
            -0.28 + rng.gen_range_f32(-0.06, 0.06)
        } else {
            0.10 + rng.gen_range_f32(-0.08, 0.08)
        };

        attrs.push(PlateAttr {
            is_ocean,
            velocity,
            base_height,
        });
    }

    attrs
}

fn apply_boundary_interactions(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    attributes: &[PlateAttr],
    height: &mut [f32],
    uplift_gain: f32,
    subduct_gain: f32,
    divergent_gain: f32,
) -> BoundaryVertices {
    let mut boundary_vertices = BoundaryVertices::new(height.len());
    let mut base_delta = vec![0.0; height.len()];
    let classify_eps = 0.05;
    let trench_eps = 0.10;

    for i in 0..positions.len() {
        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;

        for &j_u32 in &nbrs[start..end] {
            let j = j_u32 as usize;
            if j <= i {
                continue;
            }

            let pi = plate_id[i] as usize;
            let pj = plate_id[j] as usize;
            if pi == pj {
                continue;
            }

            boundary_vertices.insert(i);
            boundary_vertices.insert(j);

            let edge = sub3(positions[j], positions[i]);
            let edge_dir = normalize3(edge);
            let rel_v = sub3(attributes[pj].velocity, attributes[pi].velocity);
            let v_rel_n = dot3(rel_v, edge_dir);
            let convergent_strength = clamp((v_rel_n - classify_eps) / 0.20, 0.0, 1.0);
            let divergent_strength = clamp((-v_rel_n - classify_eps) / 0.20, 0.0, 1.0);
            let trench_strength = clamp((v_rel_n - trench_eps) / 0.18, 0.0, 1.0);

            let btype = if v_rel_n > classify_eps {
                BoundaryType::Convergent
            } else if v_rel_n < -classify_eps {
                BoundaryType::Divergent
            } else {
                BoundaryType::Transform
            };

            match btype {
                BoundaryType::Convergent => {
                    let oi = attributes[pi].is_ocean;
                    let oj = attributes[pj].is_ocean;
                    let hi = height[i];
                    let hj = height[j];
                    if oi && !oj {
                        base_delta[i] -= 0.45 * subduct_gain * trench_strength;
                        if hj > 0.10 {
                            base_delta[j] += 0.35 * uplift_gain * convergent_strength;
                        }
                    } else if !oi && oj {
                        if hi > 0.10 {
                            base_delta[i] += 0.35 * uplift_gain * convergent_strength;
                        }
                        base_delta[j] -= 0.45 * subduct_gain * trench_strength;
                    } else if !oi && !oj {
                        base_delta[i] += 0.55 * uplift_gain * convergent_strength;
                        base_delta[j] += 0.55 * uplift_gain * convergent_strength;
                    } else {
                        if v_rel_n >= 0.0 {
                            base_delta[i] -= 0.35 * subduct_gain * trench_strength;
                            base_delta[j] += 0.15 * uplift_gain * convergent_strength;
                        } else {
                            base_delta[j] -= 0.35 * subduct_gain * trench_strength;
                            base_delta[i] += 0.15 * uplift_gain * convergent_strength;
                        }
                    }
                }
                BoundaryType::Divergent => {
                    let rift_delta = -0.27 * divergent_gain * divergent_strength;
                    base_delta[i] += rift_delta;
                    base_delta[j] += rift_delta;
                }
                BoundaryType::Transform => {}
            }
        }
    }

    let sigma = 2.0;
    let w0 = 1.0;
    let w1 = (-1.0_f32 / (2.0 * sigma * sigma)).exp();
    let w2 = (-4.0_f32 / (2.0 * sigma * sigma)).exp();

    let mut spread_delta = vec![0.0; height.len()];

    for &b in &boundary_vertices.indices {
        spread_delta[b] += base_delta[b] * w0;

        let s1 = nbr_offsets[b] as usize;
        let e1 = nbr_offsets[b + 1] as usize;
        for &n1 in &nbrs[s1..e1] {
            let n1 = n1 as usize;
            spread_delta[n1] += base_delta[b] * w1;

            let s2 = nbr_offsets[n1] as usize;
            let e2 = nbr_offsets[n1 + 1] as usize;
            for &n2 in &nbrs[s2..e2] {
                let n2 = n2 as usize;
                spread_delta[n2] += base_delta[b] * w2;
            }
        }
    }

    for i in 0..height.len() {
        height[i] = clamp(height[i] + spread_delta[i], -1.0, 1.0);
    }

    boundary_vertices
}

fn smooth_heights(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    boundary_vertices: &BoundaryVertices,
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
            let boundary_scale = if boundary_vertices.mask[v] { 0.6 } else { 1.0 };
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
        let spread_factors = super::build_plate_spread_factors(plate_count, &mut rng);
        let plate_id = super::partition_plates(
            &positions,
            &phi,
            &nbr_offsets,
            &nbrs,
            &seeds,
            &spread_factors,
            params.boundary_band,
        );

        let attributes =
            super::assign_plate_attributes(plate_count, &mut rng, params.ocean_plate_ratio);
        let mut height = vec![0.0; positions.len()];
        for v in 0..positions.len() {
            let pid = plate_id[v] as usize;
            let noise = rng.gen_range_f32(-0.03, 0.03);
            height[v] = super::clamp(
                attributes[pid].base_height + 0.10 * phi[v] + noise,
                -1.2,
                1.2,
            );
        }

        let boundary_vertices = super::apply_boundary_interactions(
            &positions,
            &nbr_offsets,
            &nbrs,
            &plate_id,
            &attributes,
            &mut height,
            params.uplift_gain,
            params.subduct_gain,
            params.divergent_gain,
        );

        super::smooth_heights(
            &nbr_offsets,
            &nbrs,
            &boundary_vertices,
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

        TerrainOutput {
            height,
            plate_id,
            river_flux,
            river_next,
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
