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

