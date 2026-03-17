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

        let mut tangent = local_plate_velocity(&attributes[pid], pid, source_pos);
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

