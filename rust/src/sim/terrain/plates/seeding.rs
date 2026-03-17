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

