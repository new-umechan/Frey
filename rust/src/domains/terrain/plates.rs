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
            anisotropy: rng.gen_range_f32(0.55, 1.20),
            roughness: rng.gen_range_f32(0.02, 0.12),
            warp_weights,
            warp_gain: rng.gen_range_f32(0.06, 0.22),
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

fn local_plate_velocity(attr: &PlateAttr, plate: usize, position: [f32; 3]) -> [f32; 3] {
    let base = project_to_tangent(attr.velocity, position);
    let base_mag = length3(base);

    let blend = 0.5 + 0.5 * clamp(dot3(position, attr.drift_mix_axis), -1.0, 1.0);
    let mixed_axis = normalize3(add3(
        mul3(attr.drift_axis_primary, 1.0 - blend),
        mul3(attr.drift_axis_secondary, blend),
    ));
    let drift_axis = project_to_tangent(mixed_axis, position);
    let drift_mag = length3(drift_axis);

    let seed = plate as u32;
    let local_hash = 2.0 * trig_hash01(position, seed ^ 0x9e37_79b9) - 1.0;
    let local_scale = attr.drift_variability * local_hash;

    if drift_mag <= 1e-6 {
        return base;
    }
    let drift_dir = mul3(drift_axis, 1.0 / drift_mag);
    let mixed = add3(base, mul3(drift_dir, base_mag * local_scale));
    let tangent = project_to_tangent(mixed, position);
    if length3(tangent) <= 1e-6 {
        base
    } else {
        tangent
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
            let phi_mid: f32 = 0.5 * (phi[state.vertex] + phi[n]);
            let penalty = clamp(phi_mid.abs() / boundary_band, 0.0, 1.0);
            let profile = &growth_profiles[state.plate];
            let spread = profile.spread.max(0.35);
            let edge_dir = normalize3(sub3(positions[n], positions[state.vertex]));
            let tangent_axis =
                local_preferred_tangent_axis(profile, positions[state.vertex], edge_dir);
            let alignment = dot3(edge_dir, tangent_axis).abs();
            let directional_factor =
                1.0 + 1.25 * profile.anisotropy * (1.0 - clamp(alignment, 0.0, 1.0));
            let phi_discount = clamp(1.0 - 0.18 * phi_mid, 0.68, 1.30);
            let warp_mid = sample_plate_warp_mid(profile, plate_cost_warp_basis, state.vertex, n);
            let warp_factor = clamp(1.0 + profile.warp_gain * warp_mid, 0.82, 1.22);
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
    plate_id: &[u32],
    plate_count: usize,
    phi: &[f32],
    rng: &mut DeterministicRng,
    ocean_plate_ratio: f32,
) -> Vec<PlateAttr> {
    let mut plate_counts = vec![0usize; plate_count];
    let mut plate_phi_sum = vec![0.0f32; plate_count];
    for (v, &pid_u32) in plate_id.iter().enumerate() {
        let pid = pid_u32 as usize;
        if pid >= plate_count {
            continue;
        }
        plate_counts[pid] += 1;
        plate_phi_sum[pid] += phi[v];
    }

    let mut plate_scores = Vec::with_capacity(plate_count);
    for pid in 0..plate_count {
        let mean_phi = if plate_counts[pid] > 0 {
            plate_phi_sum[pid] / plate_counts[pid] as f32
        } else {
            0.0
        };
        let jitter = rng.gen_range_f32(-0.12, 0.12);
        plate_scores.push((pid, mean_phi + jitter, mean_phi));
    }
    plate_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

    let mut ocean_target = ((plate_count as f32) * ocean_plate_ratio).round() as usize;
    if plate_count >= 2 {
        ocean_target = ocean_target.clamp(1, plate_count - 1);
    } else {
        ocean_target = ocean_target.min(plate_count);
    }
    let continent_target = plate_count.saturating_sub(ocean_target);
    let mut is_ocean_plate = vec![true; plate_count];
    for (rank, (pid, _, _)) in plate_scores.iter().enumerate() {
        is_ocean_plate[*pid] = rank >= continent_target;
    }

    let mut attrs = Vec::with_capacity(plate_count);

    for pid in 0..plate_count {
        let is_ocean = is_ocean_plate[pid];
        let dir = rng.gen_range_f32(0.0, 2.0 * std::f32::consts::PI);
        let speed = rng.gen_range_f32(0.3, 1.0);
        let velocity = [speed * dir.cos(), speed * dir.sin(), 0.0];
        let drift_axis_primary = random_unit_vector3(rng);
        let drift_axis_secondary = random_unit_vector3(rng);
        let drift_mix_axis = random_unit_vector3(rng);
        let drift_variability = rng.gen_range_f32(0.06, 0.32);
        let mean_phi = if plate_counts[pid] > 0 {
            plate_phi_sum[pid] / plate_counts[pid] as f32
        } else {
            0.0
        };

        let base_height = if is_ocean {
            clamp(
                -0.09 + 0.02 * mean_phi + rng.gen_range_f32(-0.03, 0.02),
                -0.20,
                -0.02,
            )
        } else {
            clamp(
                0.12 + 0.05 * mean_phi + rng.gen_range_f32(-0.05, 0.06),
                0.03,
                0.30,
            )
        };
        let base_weight = if is_ocean {
            0.62 + rng.gen_range_f32(-0.06, 0.08)
        } else {
            0.22 + rng.gen_range_f32(-0.04, 0.04)
        };

        attrs.push(PlateAttr {
            is_ocean,
            velocity,
            drift_axis_primary,
            drift_axis_secondary,
            drift_mix_axis,
            drift_variability,
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
    params: &TerrainParams,
) -> Vec<VertexLithosphere> {
    const AGE_SPEED_REF: f32 = 0.65;
    const AGE_DIRECTIONAL_INFLUENCE: f32 = 0.35;

    let v_count = positions.len();
    let mut crust_age_dist = vec![f32::INFINITY; v_count];
    let mut lith = vec![
        VertexLithosphere {
            age_norm: 0.0,
            weight: 0.0,
            buoyancy: 0.0,
            competence: 0.5,
        };
        v_count
    ];
    let mut heap = BinaryHeap::new();
    let plate_count = attributes.len();
    let mut plate_age_distance_weight = vec![1.0_f32; plate_count];

    // プレートの速度に応じて重み付け
    for (pid, attr) in attributes.iter().enumerate() {
        let speed = length3(attr.velocity).max(1e-4);
        plate_age_distance_weight[pid] = AGE_SPEED_REF / speed;
    }

    let mut has_divergent_source = vec![false; plate_count];
    let mut has_boundary_seed = vec![false; plate_count];

    for i in 0..v_count {
        let pid = plate_id[i] as usize;
        lith[i].weight = attributes[pid].base_weight;
        lith[i].buoyancy = attributes[pid].base_height;
        lith[i].competence = 0.5;
    }

    let mut continental_competence_raw = vec![0.0_f32; v_count];
    for v in 0..v_count {
        let pid = plate_id[v] as usize;
        if attributes[pid].is_ocean {
            continue;
        }
        continental_competence_raw[v] = sample_continental_competence_noise(
            positions[v],
            pid as u32,
            params.continent_competence_large_scale,
            params.continent_competence_mid_scale,
        );
    }
    smooth_continental_field_by_plate(
        nbr_offsets,
        nbrs,
        plate_id,
        attributes,
        &mut continental_competence_raw,
        3,
    );

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
            let base_weight = plate_age_distance_weight[pid];

            let edge_vec = sub3(positions[n], positions[state.vertex]);
            let edge_tangent = project_to_tangent(edge_vec, positions[state.vertex]);
            let edge_dir = normalize3(edge_tangent);

            let plate_vel_tangent =
                local_plate_velocity(&attributes[pid], pid, positions[state.vertex]);
            let plate_vel_dir = normalize3(plate_vel_tangent);
            let dir_alignment = dot3(edge_dir, plate_vel_dir);
            let dir_weight = if length3(plate_vel_tangent) <= 1e-6 {
                1.0
            } else {
                clamp(1.0 - AGE_DIRECTIONAL_INFLUENCE * dir_alignment, 0.55, 1.45)
            };

            let weighted_step = step * base_weight * dir_weight;
            let next_cost = state.cost + weighted_step;
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
            let competence = clamp(
                0.5 + params.continent_competence_noise_gain * continental_competence_raw[v],
                0.0,
                1.0,
            );
            let weight = attributes[pid].base_weight
                + params.continent_competence_weight_gain * (competence - 0.5);
            lith[v].weight = weight;
            lith[v].buoyancy = attributes[pid].base_height;
            lith[v].competence = competence;
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
            competence: 0.5,
        };
    }

    lith
}

fn sample_continental_competence_noise(
    pos: [f32; 3],
    plate_seed: u32,
    large_scale: f32,
    mid_scale: f32,
) -> f32 {
    let axis_a = seeded_unit_vec(plate_seed ^ 0x85eb_ca6b);
    let axis_b = seeded_unit_vec(plate_seed ^ 0xc2b2_ae35);
    let axis_c = seeded_unit_vec(plate_seed ^ 0x27d4_eb2f);
    let phase_a = std::f32::consts::TAU * hash01_u32(plate_seed ^ 0x517c_c1b7);
    let phase_b = std::f32::consts::TAU * hash01_u32(plate_seed ^ 0x9e37_79b9);
    let phase_c = std::f32::consts::TAU * hash01_u32(plate_seed ^ 0x94d0_49bb);

    let large = (dot3(pos, axis_a) * large_scale + phase_a).sin();
    let mid_primary = (dot3(pos, axis_b) * mid_scale + phase_b).sin();
    let mid_secondary = (dot3(pos, axis_c) * (mid_scale * 1.37) + phase_c).sin();
    let mixed = 0.70 * large + 0.20 * mid_primary + 0.10 * mid_secondary;
    clamp(mixed, -1.0, 1.0)
}

fn smooth_continental_field_by_plate(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    attributes: &[PlateAttr],
    field: &mut [f32],
    iter: u32,
) {
    if iter == 0 || field.is_empty() {
        return;
    }
    let mut buf = field.to_vec();
    for _ in 0..iter {
        for v in 0..field.len() {
            let pid = plate_id[v] as usize;
            if attributes[pid].is_ocean {
                buf[v] = field[v];
                continue;
            }
            let mut sum = field[v];
            let mut wsum = 1.0_f32;
            let start = nbr_offsets[v] as usize;
            let end = nbr_offsets[v + 1] as usize;
            for &n_u32 in &nbrs[start..end] {
                let n = n_u32 as usize;
                if plate_id[n] != plate_id[v] {
                    continue;
                }
                sum += field[n];
                wsum += 1.0;
            }
            buf[v] = sum / wsum;
        }
        field.copy_from_slice(&buf);
    }
}

fn hash01_u32(seed: u32) -> f32 {
    let s = ((seed as f32) * 12.9898 + 78.233).sin();
    fract01(s * 43_758.547)
}

fn seeded_unit_vec(seed: u32) -> [f32; 3] {
    let z = 2.0 * hash01_u32(seed ^ 0x68bc_21eb) - 1.0;
    let phi = std::f32::consts::TAU * hash01_u32(seed ^ 0x02e5_be93);
    let r = (1.0 - z * z).max(0.0).sqrt();
    [r * phi.cos(), z, r * phi.sin()]
}
