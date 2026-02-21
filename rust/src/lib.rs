use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
pub struct MeshOutput {
    positions: Vec<f32>,
    indices: Vec<u32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TerrainParams {
    pub level: u32,
    pub l_max: u32,
    pub alpha: f32,
    pub num_plates_min: u32,
    pub num_plates_max: u32,
    pub ocean_plate_ratio: f32,
    pub boundary_band: f32,
    pub uplift_gain: f32,
    pub subduct_gain: f32,
    pub divergent_gain: f32,
    pub smooth_iter: u32,
    pub smooth_lambda: f32,
    pub river_rain_base: f32,
    pub river_accum_threshold: f32,
}

impl Default for TerrainParams {
    fn default() -> Self {
        Self {
            level: 6,
            l_max: 4,
            alpha: 1.5,
            num_plates_min: 8,
            num_plates_max: 18,
            ocean_plate_ratio: 0.65,
            boundary_band: 0.08,
            uplift_gain: 0.45,
            subduct_gain: 0.35,
            divergent_gain: 0.20,
            smooth_iter: 6,
            smooth_lambda: 0.35,
            river_rain_base: 0.5,
            river_accum_threshold: 0.015,
        }
    }
}

#[derive(Serialize)]
pub struct TerrainOutput {
    pub height: Vec<f32>,
    pub plate_id: Vec<u32>,
    pub river_flux: Vec<f32>,
    pub river_next: Vec<i32>,
}

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

struct DeterministicRng {
    state: u64,
    cached_normal: Option<f32>,
}

impl DeterministicRng {
    fn from_seed_bytes(seed: [u8; 16]) -> Self {
        let mut lo = [0u8; 8];
        let mut hi = [0u8; 8];
        lo.copy_from_slice(&seed[..8]);
        hi.copy_from_slice(&seed[8..]);
        let mut state = u64::from_le_bytes(lo) ^ u64::from_le_bytes(hi).rotate_left(7);
        if state == 0 {
            state = 0x9E37_79B9_7F4A_7C15;
        }
        Self {
            state,
            cached_normal: None,
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn next_f32(&mut self) -> f32 {
        let v = (self.next_u64() >> 40) as u32;
        v as f32 / 16_777_216.0
    }

    fn gen_range_f32(&mut self, min: f32, max: f32) -> f32 {
        if min >= max {
            min
        } else {
            min + (max - min) * self.next_f32()
        }
    }

    fn gen_range_u32_inclusive(&mut self, min: u32, max: u32) -> u32 {
        if min >= max {
            min
        } else {
            min + (self.next_u64() % (max - min + 1) as u64) as u32
        }
    }

    fn gen_range_usize(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            min
        } else {
            min + (self.next_u64() % (max - min) as u64) as usize
        }
    }

    fn bernoulli(&mut self, p: f32) -> bool {
        self.next_f32() < p
    }

    fn standard_normal(&mut self) -> f32 {
        if let Some(v) = self.cached_normal.take() {
            return v;
        }

        let u1 = self.next_f32().max(1e-7);
        let u2 = self.next_f32();
        let r = (-2.0 * u1.ln()).sqrt();
        let theta = 2.0 * std::f32::consts::PI * u2;
        let z0 = r * theta.cos();
        let z1 = r * theta.sin();
        self.cached_normal = Some(z1);
        z0
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

#[wasm_bindgen]
pub fn generate_mesh(level: u32) -> Result<JsValue, JsValue> {
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

#[wasm_bindgen]
pub fn generate_terrain(seed: String, params_js: JsValue) -> Result<JsValue, JsValue> {
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
        return serde_wasm_bindgen::to_value(&preset)
            .map_err(|err| JsValue::from_str(&format!("failed to serialize terrain output: {err}")));
    }

    let mut rng = rng_from_seed(&seed, &params);

    let (positions, indices) = generate_icosphere(params.level);
    let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
    let spherical = compute_spherical_coords(&positions);

    let mut phi = evaluate_phi(&spherical, params.l_max, params.alpha, &mut rng);
    normalize_zscore(&mut phi);

    let plate_count = choose_plate_count(params.num_plates_min, params.num_plates_max, &mut rng);
    let seeds = pick_plate_seeds(&phi, &positions, &nbr_offsets, &nbrs, plate_count, &mut rng);
    let mut plate_id = partition_plates(
        &positions,
        &phi,
        &nbr_offsets,
        &nbrs,
        &seeds,
        params.boundary_band,
    );
    plate_id = compact_plate_ids(plate_id, plate_count);

    let attributes = assign_plate_attributes(plate_count, &mut rng, params.ocean_plate_ratio);

    let mut height = vec![0.0; positions.len()];
    for v in 0..positions.len() {
        let pid = plate_id[v] as usize;
        let noise = rng.gen_range_f32(-0.03, 0.03);
        height[v] = clamp(attributes[pid].base_height + 0.10 * phi[v] + noise, -1.2, 1.2);
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
}

fn rng_from_seed(seed: &str, params: &TerrainParams) -> DeterministicRng {
    let canonical = format!(
        "{{\"l_max\":{},\"alpha\":{:.8},\"num_plates_min\":{},\"num_plates_max\":{},\"ocean_plate_ratio\":{:.8},\"boundary_band\":{:.8},\"uplift_gain\":{:.8},\"subduct_gain\":{:.8},\"divergent_gain\":{:.8},\"smooth_iter\":{},\"smooth_lambda\":{:.8},\"river_rain_base\":{:.8},\"river_accum_threshold\":{:.8}}}",
        params.l_max,
        params.alpha,
        params.num_plates_min,
        params.num_plates_max,
        params.ocean_plate_ratio,
        params.boundary_band,
        params.uplift_gain,
        params.subduct_gain,
        params.divergent_gain,
        params.smooth_iter,
        params.smooth_lambda,
        params.river_rain_base,
        params.river_accum_threshold,
    );

    let mut source = Vec::new();
    source.extend_from_slice(seed.as_bytes());
    source.extend_from_slice(canonical.as_bytes());
    let digest = pseudo_sha256(&source);

    let mut seed16 = [0u8; 16];
    seed16.copy_from_slice(&digest[..16]);
    DeterministicRng::from_seed_bytes(seed16)
}

fn pseudo_sha256(input: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for i in 0..4u64 {
        let mut block = Vec::with_capacity(input.len() + 8);
        block.extend_from_slice(input);
        block.extend_from_slice(&i.to_le_bytes());
        let h = fnv1a64(&block).wrapping_add(0x9E37_79B9_7F4A_7C15_u64.wrapping_mul(i + 1));
        out[(i as usize) * 8..(i as usize + 1) * 8].copy_from_slice(&h.to_le_bytes());
    }
    out
}

fn fnv1a64(input: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for b in input {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3_u64);
    }
    hash
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
        let p_next = (((2 * ll - 1) as f32) * x * p_curr - ((ll + m - 1) as f32) * p_prev)
            / (ll - m) as f32;
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

fn partition_plates(
    positions: &[[f32; 3]],
    phi: &[f32],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    seeds: &[usize],
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
            let next_cost = state.cost + edge_len * (1.0 + penalty);

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
            -0.45 + rng.gen_range_f32(-0.08, 0.08)
        } else {
            0.18 + rng.gen_range_f32(-0.10, 0.10)
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
) -> HashSet<usize> {
    let mut boundary_vertices = HashSet::<usize>::new();
    let mut base_delta = vec![0.0; height.len()];
    let eps = 0.02;

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

            let btype = if v_rel_n > eps {
                BoundaryType::Convergent
            } else if v_rel_n < -eps {
                BoundaryType::Divergent
            } else {
                BoundaryType::Transform
            };

            match btype {
                BoundaryType::Convergent => {
                    let oi = attributes[pi].is_ocean;
                    let oj = attributes[pj].is_ocean;
                    if oi && !oj {
                        base_delta[i] -= subduct_gain;
                        base_delta[j] += uplift_gain;
                    } else if !oi && oj {
                        base_delta[i] += uplift_gain;
                        base_delta[j] -= subduct_gain;
                    } else if !oi && !oj {
                        base_delta[i] += 0.7 * uplift_gain;
                        base_delta[j] += 0.7 * uplift_gain;
                    } else {
                        if v_rel_n >= 0.0 {
                            base_delta[i] -= 0.7 * subduct_gain;
                            base_delta[j] += 0.3 * uplift_gain;
                        } else {
                            base_delta[j] -= 0.7 * subduct_gain;
                            base_delta[i] += 0.3 * uplift_gain;
                        }
                    }
                }
                BoundaryType::Divergent => {
                    base_delta[i] -= divergent_gain;
                    base_delta[j] -= divergent_gain;
                    base_delta[i] += 0.075 * divergent_gain;
                    base_delta[j] += 0.075 * divergent_gain;
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

    for &b in &boundary_vertices {
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
    boundary_vertices: &HashSet<usize>,
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

            let lambda = if boundary_vertices.contains(&v) {
                smooth_lambda * 0.6
            } else {
                smooth_lambda
            };
            buffer[v] = clamp(height[v] + lambda * (mean - height[v]), -1.0, 1.0);
        }
        height.copy_from_slice(&buffer);
    }
}

fn generate_rivers(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    river_rain_base: f32,
    river_accum_threshold: f32,
) -> (Vec<f32>, Vec<i32>) {
    let v_count = positions.len();
    let mut rain = vec![0.0; v_count];
    let mut river_next = vec![-1; v_count];
    let mut river_flux = vec![0.0; v_count];

    for i in 0..v_count {
        let lat = clamp(positions[i][1], -1.0, 1.0).asin();
        rain[i] = river_rain_base * (1.0 - lat.abs() / (std::f32::consts::PI * 0.5)).max(0.0);
    }

    for i in 0..v_count {
        if height[i] <= 0.0 {
            river_next[i] = -1;
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
    order.sort_by(|a, b| height[*b].partial_cmp(&height[*a]).unwrap_or(Ordering::Equal));

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

    for i in 0..v_count {
        river_flux[i] /= max_flux;
        if river_flux[i] < river_accum_threshold {
            river_flux[i] = 0.0;
        }
        if height[i] <= 0.0 {
            river_next[i] = -1;
        }
    }

    (river_flux, river_next)
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

fn flatten_positions(positions: &[[f32; 3]]) -> Vec<f32> {
    positions
        .iter()
        .flat_map(|v| [v[0], v[1], v[2]])
        .collect::<Vec<f32>>()
}

fn build_neighbors(vertex_count: usize, indices: &[u32]) -> (Vec<u32>, Vec<u32>) {
    let mut adj = vec![Vec::<u32>::new(); vertex_count];

    for tri in indices.chunks_exact(3) {
        let a = tri[0];
        let b = tri[1];
        let c = tri[2];
        add_undirected_edge(&mut adj, a, b);
        add_undirected_edge(&mut adj, b, c);
        add_undirected_edge(&mut adj, c, a);
    }

    for list in &mut adj {
        list.sort_unstable();
        list.dedup();
    }

    let mut offsets = Vec::with_capacity(vertex_count + 1);
    offsets.push(0);
    let mut nbrs = Vec::new();
    for list in adj {
        nbrs.extend(list.iter().copied());
        offsets.push(nbrs.len() as u32);
    }

    (offsets, nbrs)
}

fn add_undirected_edge(adj: &mut [Vec<u32>], a: u32, b: u32) {
    adj[a as usize].push(b);
    adj[b as usize].push(a);
}

fn generate_icosphere(level: u32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let phi = (1.0 + 5.0_f32.sqrt()) / 2.0;
    let mut positions = vec![
        [-1.0, phi, 0.0],
        [1.0, phi, 0.0],
        [-1.0, -phi, 0.0],
        [1.0, -phi, 0.0],
        [0.0, -1.0, phi],
        [0.0, 1.0, phi],
        [0.0, -1.0, -phi],
        [0.0, 1.0, -phi],
        [phi, 0.0, -1.0],
        [phi, 0.0, 1.0],
        [-phi, 0.0, -1.0],
        [-phi, 0.0, 1.0],
    ];

    for vertex in &mut positions {
        normalize(vertex);
    }

    let mut indices: Vec<u32> = vec![
        0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11, 1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7,
        6, 7, 1, 8, 3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9, 4, 9, 5, 2, 4, 11, 6, 2, 10,
        8, 6, 7, 9, 8, 1,
    ];

    for _ in 0..level {
        let mut midpoint_cache = HashMap::<(u32, u32), u32>::new();
        let mut subdivided_indices = Vec::with_capacity(indices.len() * 4);

        for triangle in indices.chunks_exact(3) {
            let i0 = triangle[0];
            let i1 = triangle[1];
            let i2 = triangle[2];

            let a = midpoint_index(i0, i1, &mut positions, &mut midpoint_cache);
            let b = midpoint_index(i1, i2, &mut positions, &mut midpoint_cache);
            let c = midpoint_index(i2, i0, &mut positions, &mut midpoint_cache);

            subdivided_indices.extend_from_slice(&[i0, a, c]);
            subdivided_indices.extend_from_slice(&[i1, b, a]);
            subdivided_indices.extend_from_slice(&[i2, c, b]);
            subdivided_indices.extend_from_slice(&[a, b, c]);
        }

        indices = subdivided_indices;
    }

    (positions, indices)
}

fn midpoint_index(
    i0: u32,
    i1: u32,
    positions: &mut Vec<[f32; 3]>,
    midpoint_cache: &mut HashMap<(u32, u32), u32>,
) -> u32 {
    let key = if i0 < i1 { (i0, i1) } else { (i1, i0) };
    if let Some(index) = midpoint_cache.get(&key) {
        return *index;
    }

    let v0 = positions[i0 as usize];
    let v1 = positions[i1 as usize];

    let mut midpoint = [
        (v0[0] + v1[0]) * 0.5,
        (v0[1] + v1[1]) * 0.5,
        (v0[2] + v1[2]) * 0.5,
    ];
    normalize(&mut midpoint);

    let index = positions.len() as u32;
    positions.push(midpoint);
    midpoint_cache.insert(key, index);
    index
}

fn normalize(v: &mut [f32; 3]) {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length > 0.0 {
        v[0] /= length;
        v[1] /= length;
        v[2] /= length;
    }
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn length3(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = length3(v);
    if len <= 1e-6 {
        [0.0, 0.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

fn chord_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    length3(sub3(a, b))
}

fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::{
        generate_icosphere, generate_rivers, normalize_zscore, rng_from_seed, TerrainOutput, TerrainParams,
    };

    fn generate_for_test(seed: &str, params: &TerrainParams) -> TerrainOutput {
        let mut rng = rng_from_seed(seed, params);
        let (positions, indices) = generate_icosphere(params.level);
        let (nbr_offsets, nbrs) = super::build_neighbors(positions.len(), &indices);
        let spherical = super::compute_spherical_coords(&positions);

        let mut phi = super::evaluate_phi(&spherical, params.l_max, params.alpha, &mut rng);
        normalize_zscore(&mut phi);
        let plate_count = super::choose_plate_count(params.num_plates_min, params.num_plates_max, &mut rng);
        let seeds = super::pick_plate_seeds(
            &phi,
            &positions,
            &nbr_offsets,
            &nbrs,
            plate_count,
            &mut rng,
        );
        let plate_id = super::partition_plates(
            &positions,
            &phi,
            &nbr_offsets,
            &nbrs,
            &seeds,
            params.boundary_band,
        );

        let attributes = super::assign_plate_attributes(plate_count, &mut rng, params.ocean_plate_ratio);
        let mut height = vec![0.0; positions.len()];
        for v in 0..positions.len() {
            let pid = plate_id[v] as usize;
            let noise = rng.gen_range_f32(-0.03, 0.03);
            height[v] = super::clamp(attributes[pid].base_height + 0.10 * phi[v] + noise, -1.2, 1.2);
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
}
