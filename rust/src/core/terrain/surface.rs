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
    plate_id: &[u32],
    attributes: &[PlateAttr],
    target_sea_ratio: f32,
) {
    let mut adjusted = Vec::with_capacity(height.len());
    for v in 0..height.len() {
        let pid = plate_id[v] as usize;
        let buoyancy_bias = if attributes[pid].is_ocean { -0.09 } else { 0.09 };
        adjusted.push(height[v] + buoyancy_bias);
    }

    let mut sorted = adjusted.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let sea_idx = ((sorted.len() as f32) * target_sea_ratio) as usize;
    let sea_idx = sea_idx.min(sorted.len().saturating_sub(1));
    let sea_level = sorted[sea_idx];

    for v in 0..height.len() {
        let normalized = (adjusted[v] - sea_level) * 0.58;
        height[v] = clamp(normalized, -1.0, 1.0);
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

fn apply_hotspot_island_chains(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    attributes: &[PlateAttr],
    height: &mut [f32],
    rng: &mut DeterministicRng,
) {
    let mut ocean_interior = Vec::new();
    for v in 0..positions.len() {
        let pid = plate_id[v] as usize;
        if pid >= attributes.len() || !attributes[pid].is_ocean {
            continue;
        }
        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;
        if end <= start {
            continue;
        }
        let mut same_plate_neighbors = 0usize;
        for &n in &nbrs[start..end] {
            if plate_id[n as usize] == plate_id[v] {
                same_plate_neighbors += 1;
            }
        }
        if same_plate_neighbors + 1 >= end - start && height[v] < -0.03 {
            ocean_interior.push(v);
        }
    }

    if ocean_interior.is_empty() {
        return;
    }

    let mut ocean_plate_count = 0usize;
    for attr in attributes {
        if attr.is_ocean {
            ocean_plate_count += 1;
        }
    }
    let track_target = ((ocean_plate_count as f32) * 0.65).round() as usize;
    let track_target = track_target.clamp(2, 8);

    let mut chosen_sources = Vec::<usize>::new();
    let mut attempts = 0usize;
    while chosen_sources.len() < track_target && attempts < ocean_interior.len() * 5 {
        attempts += 1;
        let candidate = ocean_interior[rng.gen_range_usize(0, ocean_interior.len())];
        if chosen_sources
            .iter()
            .any(|&s| chord_distance(positions[s], positions[candidate]) < 0.30)
        {
            continue;
        }
        chosen_sources.push(candidate);
    }
    if chosen_sources.is_empty() {
        chosen_sources.push(ocean_interior[rng.gen_range_usize(0, ocean_interior.len())]);
    }

    for &source in &chosen_sources {
        let pid = plate_id[source] as usize;
        let source_pos = positions[source];

        let mut tangent = project_to_tangent(attributes[pid].velocity, source_pos);
        if length3(tangent) <= 1e-5 {
            tangent = project_to_tangent(random_unit_vector3(rng), source_pos);
        }
        if length3(tangent) <= 1e-5 {
            continue;
        }
        tangent = normalize3(tangent);
        // ホットスポットは固定、プレート移動で島列が伸びる想定なので、
        // 進行方向を一方に固定して直線性を高める。
        tangent = mul3(tangent, -1.0);

        let segment_count = rng.gen_range_u32_inclusive(7, 13) as usize;
        let step_size = rng.gen_range_f32(0.080, 0.125);
        let plume_width = rng.gen_range_f32(0.040, 0.065);
        let along_sigma = rng.gen_range_f32(0.016, 0.026);
        let cross_sigma = rng.gen_range_f32(0.008, 0.014);
        let plume_amp = rng.gen_range_f32(0.08, 0.18);
        let peak_amp = rng.gen_range_f32(0.18, 0.34);

        let mut centers = Vec::<([f32; 3], [f32; 3], f32)>::with_capacity(segment_count);
        let mut current_pos = source_pos;
        let mut current_tangent = tangent;
        for idx in 0..segment_count {
            let age_t = if segment_count <= 1 {
                0.0
            } else {
                idx as f32 / (segment_count - 1) as f32
            };
            let age_decay = (1.0 - age_t).powf(1.75);
            centers.push((current_pos, current_tangent, age_decay));

            let next_pos = normalize3(add3(current_pos, mul3(current_tangent, step_size)));
            current_tangent = normalize3(project_to_tangent(current_tangent, next_pos));
            if length3(current_tangent) <= 1e-5 {
                break;
            }
            current_pos = next_pos;
        }

        for v in 0..positions.len() {
            if plate_id[v] as usize != pid {
                continue;
            }
            if height[v] > 0.20 {
                continue;
            }

            let mut uplift = 0.0f32;
            for (idx, (center, center_tangent, age_decay)) in centers.iter().enumerate() {
                let d = chord_distance(positions[v], *center);
                if d > 0.10 {
                    continue;
                }

                let tangent_delta = project_to_tangent(sub3(positions[v], *center), *center);
                let along = dot3(tangent_delta, *center_tangent);
                let across = length3(sub3(tangent_delta, mul3(*center_tangent, along)));

                let plume = if idx == 0 {
                    let plume_shape = (-(d * d) / (2.0 * plume_width * plume_width)).exp();
                    plume_amp * plume_shape
                } else {
                    0.0
                };

                let island_shape = (-(0.5
                    * ((along / along_sigma) * (along / along_sigma)
                        + (across / cross_sigma) * (across / cross_sigma))))
                    .exp();
                let segmentation = if idx == 0 { 1.0 } else { 0.90 };
                uplift += plume + peak_amp * *age_decay * segmentation * island_shape;
            }

            if uplift <= 0.0 {
                continue;
            }

            let ocean_bias = clamp((-height[v] + 0.12) / 0.24, 0.90, 2.00);
            let mut new_h = height[v] + uplift * ocean_bias;
            if uplift > 0.20 && height[v] > -0.24 {
                new_h += 0.04;
            }
            height[v] = clamp(new_h, -1.0, 1.0);
        }
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
    let (spill_level, spill_steps, overflow_parent) =
        compute_overflow_route_keys(positions, nbr_offsets, nbrs, height);
    let mut river_next = vec![-1; v_count];
    let mut river_flux = vec![0.0; v_count];

    for i in 0..v_count {
        if height[i] <= 0.0 {
            continue;
        }

        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;

        let mut best = -1;
        let mut best_drop = -f32::INFINITY;
        let mut best_is_downhill = false;
        let mut best_spill_level = f32::INFINITY;
        let mut best_spill_steps = u32::MAX;

        for &n in &nbrs[start..end] {
            let n = n as usize;
            if !flow_key_strictly_decreases(
                spill_level[i],
                spill_steps[i],
                spill_level[n],
                spill_steps[n],
            ) {
                continue;
            }

            let drop = height[i] - height[n];
            let is_downhill = drop > 0.0;
            let better = if best < 0 {
                true
            } else if is_downhill != best_is_downhill {
                is_downhill
            } else if is_downhill {
                drop > best_drop + 1e-6
            } else if spill_level[n] + 1e-6 < best_spill_level {
                true
            } else if (spill_level[n] - best_spill_level).abs() <= 1e-6 {
                spill_steps[n] < best_spill_steps
            } else {
                false
            };

            if better {
                best = n as i32;
                best_drop = drop;
                best_is_downhill = is_downhill;
                best_spill_level = spill_level[n];
                best_spill_steps = spill_steps[n];
            }
        }

        river_next[i] = if best >= 0 { best } else { overflow_parent[i] };
    }

    let mut order = (0..v_count).collect::<Vec<_>>();
    order.sort_by(|a, b| {
        spill_level[*b]
            .partial_cmp(&spill_level[*a])
            .unwrap_or(Ordering::Equal)
            .then_with(|| spill_steps[*b].cmp(&spill_steps[*a]))
            .then_with(|| {
                height[*b]
                    .partial_cmp(&height[*a])
                    .unwrap_or(Ordering::Equal)
            })
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

#[derive(Clone, Copy)]
struct FlowRouteState {
    vertex: usize,
    spill_level: f32,
    steps: u32,
}

impl Ord for FlowRouteState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .spill_level
            .partial_cmp(&self.spill_level)
            .unwrap_or(Ordering::Equal)
            .then_with(|| other.steps.cmp(&self.steps))
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}

impl PartialOrd for FlowRouteState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for FlowRouteState {
    fn eq(&self, other: &Self) -> bool {
        self.vertex == other.vertex
            && self.steps == other.steps
            && (self.spill_level - other.spill_level).abs() <= 1e-6
    }
}

impl Eq for FlowRouteState {}

fn flow_key_better(level_a: f32, steps_a: u32, level_b: f32, steps_b: u32) -> bool {
    level_a + 1e-6 < level_b || ((level_a - level_b).abs() <= 1e-6 && steps_a < steps_b)
}

fn flow_key_strictly_decreases(
    from_level: f32,
    from_steps: u32,
    to_level: f32,
    to_steps: u32,
) -> bool {
    flow_key_better(to_level, to_steps, from_level, from_steps)
}

fn compute_overflow_route_keys(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
) -> (Vec<f32>, Vec<u32>, Vec<i32>) {
    let v_count = positions.len();
    let _ = positions;
    let mut spill_level = vec![f32::INFINITY; v_count];
    let mut spill_steps = vec![u32::MAX; v_count];
    let mut overflow_parent = vec![-1; v_count];
    let mut heap = BinaryHeap::<FlowRouteState>::new();

    for i in 0..v_count {
        if height[i] <= 0.0 {
            spill_level[i] = height[i];
            spill_steps[i] = 0;
            heap.push(FlowRouteState {
                vertex: i,
                spill_level: height[i],
                steps: 0,
            });
        }
    }

    while let Some(state) = heap.pop() {
        let i = state.vertex;
        if (state.spill_level - spill_level[i]).abs() > 1e-6 || state.steps != spill_steps[i] {
            continue;
        }

        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        for &n in &nbrs[start..end] {
            let n = n as usize;
            let cand_level = state.spill_level.max(height[n]);
            let cand_steps = state.steps.saturating_add(1);
            if flow_key_better(cand_level, cand_steps, spill_level[n], spill_steps[n]) {
                spill_level[n] = cand_level;
                spill_steps[n] = cand_steps;
                overflow_parent[n] = i as i32;
                heap.push(FlowRouteState {
                    vertex: n,
                    spill_level: cand_level,
                    steps: cand_steps,
                });
            }
        }
    }

    (spill_level, spill_steps, overflow_parent)
}

fn compute_lake_depth_map(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
) -> Vec<f32> {
    let (spill_level, _spill_steps, _overflow_parent) =
        compute_overflow_route_keys(positions, nbr_offsets, nbrs, height);
    let mut lake_depth = vec![0.0; height.len()];

    for i in 0..height.len() {
        if height[i] <= 0.0 {
            continue;
        }
        let depth = (spill_level[i] - height[i]).max(0.0);
        if depth > 1e-4 {
            lake_depth[i] = depth;
        }
    }

    lake_depth
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
    let lake_depth = compute_lake_depth_map(positions, nbr_offsets, nbrs, &height);

    TerrainOutput {
        height,
        plate_id,
        river_flux,
        river_next,
        lake_depth,
        vertex_weight: vec![0.66, 0.24, 0.20, 0.61],
        plate_is_ocean: vec![1, 0, 0, 1],
        plate_base_height: vec![-0.06, 0.14, 0.08, -0.03],
        plate_base_weight: vec![0.66, 0.24, 0.20, 0.61],
        debug_trench_strength: vec![0.0; positions.len()],
        debug_arc_strength: vec![0.0; positions.len()],
        debug_backarc_strength: vec![0.0; positions.len()],
        debug_ocean_ocean_arc_strength: vec![0.0; positions.len()],
    }
}
