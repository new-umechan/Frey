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
    vertex_competence: &[f32],
    height: &mut [f32],
    params: &TerrainParams,
) {
    if params.erosion_iterations == 0
        || params.hydraulic_erosion_rate <= 0.0
        || params.erosion_max_delta_per_iter <= 0.0
    {
        return;
    }

    let v_count = height.len();
    let mut next_height = height.to_vec();
    let mut delta = vec![0.0; v_count];
    let mut deposition_armor = vec![0.0; v_count];
    let mut sediment_in = vec![0.0; v_count];

    for _ in 0..params.erosion_iterations {
        let (river_flux, river_next) = compute_river_flux_and_next(
            positions,
            nbr_offsets,
            nbrs,
            height,
            params.river_rain_base,
        );

        delta.fill(0.0);
        sediment_in.fill(0.0);
        for armor in &mut deposition_armor {
            *armor *= 0.82;
        }

        let order = sorted_vertices_by_height_desc(height);
        for &i in &order {
            let next = river_next[i];
            let h_i = height[i];
            let mut sediment = sediment_in[i];

            let (next_idx, next_h) = if next >= 0 {
                let n = next as usize;
                (Some(n), height[n])
            } else {
                (None, -1.0)
            };

            let (local_slope, downstream_slope, flattening) = if let Some(n) = next_idx {
                let edge_len = chord_distance(positions[i], positions[n]).max(1e-4);
                let raw_drop = (h_i - height[n]).max(0.0);
                let local_slope = raw_drop / edge_len;
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
                (local_slope, downstream_slope, flattening)
            } else {
                (0.0, 0.0, 0.0)
            };

            let openness = local_open_basin_factor(nbr_offsets, nbrs, height, i);
            let source_is_coastal = is_coastal_cell(nbr_offsets, nbrs, height, i);
            let shallow_factor = if h_i <= 0.0 && h_i > params.shallow_sea_floor {
                let depth = clamp(-h_i, 0.0, (0.0 - params.shallow_sea_floor).max(1e-4));
                1.0 - depth / (0.0 - params.shallow_sea_floor).max(1e-4)
            } else {
                0.0
            };
            let estuary_factor = if h_i > 0.0 && next_h <= 0.0 && next_idx.is_some() {
                1.0
            } else if h_i <= 0.0 && source_is_coastal && sediment > 0.0 {
                0.65
            } else {
                0.0
            };

            let flux_term = river_flux[i].max(1e-4).powf(0.85);
            let slope_term = local_slope.max(params.erosion_min_slope).powf(0.70);
            let transport_context = clamp(
                1.0 + 0.20 * (1.0 - flattening) - 0.35 * openness - 0.55 * estuary_factor
                    + 0.10 * downstream_slope,
                0.15,
                2.5,
            );
            let capacity = params.sediment_capacity_gain * flux_term * slope_term * transport_context;

            if h_i > 0.0 {
                let competence = vertex_competence.get(i).copied().unwrap_or(0.5);
                let inverse_comp = 1.0 - clamp(competence, 0.0, 1.0);
                let erodibility = lerp(
                    1.0,
                    inverse_comp,
                    clamp(params.continent_erodibility_from_competence, 0.0, 1.0),
                );
                let armor_factor = 1.0 - 0.60 * clamp(deposition_armor[i], 0.0, 1.0);
                let erosion_demand = (capacity - sediment).max(0.0);
                let erode_amount = clamp(
                    params.hydraulic_erosion_rate * erosion_demand * erodibility * armor_factor,
                    0.0,
                    params.erosion_max_delta_per_iter,
                );
                if erode_amount > 0.0 {
                    delta[i] -= erode_amount;
                    sediment += erode_amount;
                }
            }

            let deep_sea_loss_factor = if h_i <= params.shallow_sea_floor {
                0.65
            } else if h_i <= 0.0 {
                0.12 * (1.0 - shallow_factor)
            } else {
                0.0
            };

            let overload = (sediment - capacity).max(0.0);
            if overload > 0.0 {
                let deposit_context = clamp(
                    1.0
                        + 0.85 * flattening
                        + 0.55 * openness
                        + 0.85 * estuary_factor
                        + 0.75 * shallow_factor
                        + if next_idx.is_none() && h_i > 0.0 { 0.8 } else { 0.0 },
                    0.3,
                    4.0,
                );
                let deposit_cap = if h_i <= 0.0 {
                    params.erosion_max_delta_per_iter * 2.0
                } else {
                    params.erosion_max_delta_per_iter * 1.25
                };
                let deposit_amount = clamp(
                    params.hydraulic_deposit_rate * overload * deposit_context,
                    0.0,
                    sediment.min(deposit_cap.max(0.0)),
                );
                if deposit_amount > 0.0 {
                    distribute_deposition_by_context(
                        nbr_offsets,
                        nbrs,
                        height,
                        &mut delta,
                        &mut deposition_armor,
                        params,
                        i,
                        deposit_amount,
                        flattening,
                        openness,
                        estuary_factor,
                        shallow_factor,
                    );
                    sediment -= deposit_amount;
                }
            }

            if deep_sea_loss_factor > 0.0 && sediment > 0.0 {
                let loss = sediment * deep_sea_loss_factor;
                sediment = (sediment - loss).max(0.0);
                if h_i <= params.shallow_sea_floor && sediment > 0.0 {
                    let residual_deep_deposit =
                        clamp(sediment * 0.20, 0.0, params.erosion_max_delta_per_iter * 0.75);
                    if residual_deep_deposit > 0.0 {
                        distribute_deposition_by_context(
                            nbr_offsets,
                            nbrs,
                            height,
                            &mut delta,
                            &mut deposition_armor,
                            params,
                            i,
                            residual_deep_deposit,
                            0.2,
                            0.1,
                            0.0,
                            0.0,
                        );
                        sediment -= residual_deep_deposit;
                    }
                }
            }

            if let Some(n) = next_idx {
                sediment_in[n] += sediment;
            }
        }

        for i in 0..v_count {
            next_height[i] = clamp(height[i] + delta[i], -1.2, 1.2);
        }
        height.copy_from_slice(&next_height);
    }
}

fn sorted_vertices_by_height_desc(height: &[f32]) -> Vec<usize> {
    let mut order = (0..height.len()).collect::<Vec<_>>();
    order.sort_by(|&a, &b| {
        height[b]
            .partial_cmp(&height[a])
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.cmp(&b))
    });
    order
}

fn is_coastal_cell(nbr_offsets: &[u32], nbrs: &[u32], height: &[f32], v: usize) -> bool {
    let h = height[v];
    let start = nbr_offsets[v] as usize;
    let end = nbr_offsets[v + 1] as usize;
    nbrs[start..end].iter().any(|&n_u32| {
        let nh = height[n_u32 as usize];
        (h > 0.0 && nh <= 0.0) || (h <= 0.0 && nh > 0.0)
    })
}

fn local_open_basin_factor(nbr_offsets: &[u32], nbrs: &[u32], height: &[f32], v: usize) -> f32 {
    let start = nbr_offsets[v] as usize;
    let end = nbr_offsets[v + 1] as usize;
    if end <= start {
        return 0.0;
    }

    let h = height[v];
    let mut openness_sum = 0.0f32;
    let mut count = 0usize;
    for &n_u32 in &nbrs[start..end] {
        let nh = height[n_u32 as usize];
        let same_band = 1.0 - clamp((nh - h).abs() / 0.08, 0.0, 1.0);
        let not_blocked = if nh <= h + 0.02 { 1.0 } else { 0.25 };
        openness_sum += (0.25 + 0.75 * same_band) * not_blocked;
        count += 1;
    }

    clamp(openness_sum / (count as f32), 0.0, 1.0)
}

fn apply_deposit_to_cell(
    delta: &mut [f32],
    deposition_armor: &mut [f32],
    params: &TerrainParams,
    v: usize,
    amount: f32,
) {
    if amount <= 0.0 {
        return;
    }
    delta[v] += amount;
    let armor_gain = clamp(
        amount / params.erosion_max_delta_per_iter.max(1e-6),
        0.0,
        1.0,
    );
    deposition_armor[v] = clamp(deposition_armor[v] + 0.55 * armor_gain, 0.0, 1.0);
}

fn distribute_deposition_by_context(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    delta: &mut [f32],
    deposition_armor: &mut [f32],
    params: &TerrainParams,
    center: usize,
    amount: f32,
    flattening: f32,
    openness: f32,
    estuary_factor: f32,
    shallow_factor: f32,
) {
    if amount <= 0.0 {
        return;
    }

    let spread_strength = clamp(
        0.10 + 0.45 * flattening + 0.35 * openness + 0.35 * estuary_factor + 0.30 * shallow_factor,
        0.0,
        0.92,
    );
    let center_share = 1.0 - spread_strength;
    let mut center_amount = amount * center_share;

    // 河口遷移では核を残しつつ、背後の海岸セルにも少量返す。
    if estuary_factor > 0.0 && height[center] <= 0.0 {
        center_amount += amount * 0.08 * estuary_factor;
    }

    apply_deposit_to_cell(delta, deposition_armor, params, center, center_amount.min(amount));

    let spread_pool = (amount - center_amount.min(amount)).max(0.0);
    if spread_pool <= 1e-8 {
        return;
    }

    let start = nbr_offsets[center] as usize;
    let end = nbr_offsets[center + 1] as usize;
    if end <= start {
        apply_deposit_to_cell(delta, deposition_armor, params, center, spread_pool);
        return;
    }

    let center_h = height[center];
    let shallow_range = (0.0 - params.shallow_sea_floor).max(1e-4);
    let mut weight_sum = 0.0f32;
    let mut weights = Vec::<(usize, f32)>::with_capacity(end - start);

    for &m_u32 in &nbrs[start..end] {
        let m = m_u32 as usize;
        let mh = height[m];
        let same_band = 1.0 - clamp((mh - center_h).abs() / 0.08, 0.0, 1.0);
        let lower_pref = if mh <= center_h { 1.0 } else { 0.30 };
        let marine_pref = if mh <= 0.0 { 1.0 } else { 0.40 };
        let shallow_pref = if mh <= 0.0 && mh > params.shallow_sea_floor {
            let depth = clamp(-mh, 0.0, shallow_range);
            0.35 + 0.65 * (1.0 - depth / shallow_range)
        } else if mh > 0.0 {
            0.25
        } else {
            0.10
        };

        let weight = (0.05 + 0.55 * lower_pref + 0.35 * same_band)
            * (1.0 + 0.70 * openness * same_band)
            * (1.0 + 0.90 * estuary_factor * marine_pref)
            * (1.0 + 1.10 * shallow_factor * shallow_pref);
        if weight <= 0.0 {
            continue;
        }
        weight_sum += weight;
        weights.push((m, weight));
    }

    if weight_sum <= 1e-8 {
        apply_deposit_to_cell(delta, deposition_armor, params, center, spread_pool);
        return;
    }

    for (m, w) in weights {
        apply_deposit_to_cell(delta, deposition_armor, params, m, spread_pool * (w / weight_sum));
    }
}

pub(crate) fn init_async_erosion_automaton(
    seed: &str,
    mut params: TerrainParams,
) -> crate::ErosionAutomatonState {
    sanitize_params(&mut params);

    let terrain = generate(seed, params.clone());
    let (positions, indices) = generate_icosphere(params.level);
    let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
    let rain = build_precipitation_map(
        &positions,
        &nbr_offsets,
        &nbrs,
        &terrain.height,
        params.river_rain_base,
    );

    let v_count = terrain.height.len();
    let active_queue = (0..v_count as u32).collect::<Vec<_>>();
    let mut in_queue = vec![0u8; v_count];
    in_queue.fill(1);

    crate::ErosionAutomatonState {
        positions,
        nbr_offsets,
        nbrs,
        height: terrain.height,
        water: vec![0.0; v_count],
        sediment: vec![0.0; v_count],
        armor: vec![0.0; v_count],
        rain,
        river_flux: terrain.river_flux,
        river_next: terrain.river_next,
        active_queue,
        active_head: 0,
        in_queue,
        rain_cursor: 0,
        tick: 0,
        recent_changed: Vec::new(),
        params,
    }
}

pub(crate) fn step_async_erosion_automaton(
    state: &mut crate::ErosionAutomatonState,
    budget_cells: u32,
) {
    let v_count = state.height.len();
    if v_count == 0 {
        return;
    }
    if state.positions.len() != v_count
        || state.nbr_offsets.len() != v_count + 1
        || state.water.len() != v_count
        || state.sediment.len() != v_count
        || state.armor.len() != v_count
        || state.rain.len() != v_count
        || state.river_flux.len() != v_count
        || state.river_next.len() != v_count
        || state.in_queue.len() != v_count
    {
        return;
    }

    let budget = budget_cells.max(1) as usize;
    state.tick = state.tick.saturating_add(1);
    state.recent_changed.clear();

    let mut changed_mark = vec![0u8; v_count];
    let rain_inject_count = ((budget / 2).clamp(16, 256)).min(v_count);
    inject_async_rain(state, rain_inject_count, &mut changed_mark);

    let mut processed = 0usize;
    while processed < budget {
        let Some(v) = pop_active_vertex(state) else {
            break;
        };
        processed += 1;

        let result = process_async_erosion_cell(state, v);
        if result.changed {
            mark_changed_vertex(v, &mut changed_mark, &mut state.recent_changed);
        }
        if result.deposited_here {
            enqueue_neighbors(state, v);
            mark_neighbors_changed(
                &state.nbr_offsets,
                &state.nbrs,
                v,
                &mut changed_mark,
                &mut state.recent_changed,
            );
        }
        if let Some(n) = result.downstream {
            enqueue_active_vertex(state, n);
            if result.changed {
                mark_changed_vertex(n, &mut changed_mark, &mut state.recent_changed);
            }
        }
        if result.changed {
            enqueue_active_vertex(state, v);
        }
    }

    compact_active_queue(state);
}

#[derive(Default)]
struct AsyncCellUpdateResult {
    changed: bool,
    deposited_here: bool,
    downstream: Option<usize>,
}

fn process_async_erosion_cell(
    state: &mut crate::ErosionAutomatonState,
    i: usize,
) -> AsyncCellUpdateResult {
    let mut result = AsyncCellUpdateResult::default();
    let params = &state.params;
    let h_i_before = state.height[i];
    let water_before = state.water[i];
    let sediment_before = state.sediment[i];
    let river_next_before = state.river_next[i];
    let river_flux_before = state.river_flux[i];

    state.armor[i] *= 0.985;

    let (next_idx, local_slope, next_h) = find_local_flow_target(
        &state.positions,
        &state.nbr_offsets,
        &state.nbrs,
        &state.height,
        i,
    );
    let downstream_slope = if let Some(n) = next_idx {
        estimate_local_outflow_slope(
            &state.positions,
            &state.nbr_offsets,
            &state.nbrs,
            &state.height,
            n,
        )
    } else {
        0.0
    };
    let flattening = clamp(
        1.0 - downstream_slope / (local_slope.max(params.erosion_min_slope) + 1e-6),
        0.0,
        1.0,
    );
    let h_i = state.height[i];
    let source_is_coastal = is_coastal_cell(&state.nbr_offsets, &state.nbrs, &state.height, i);
    let openness = local_open_basin_factor(&state.nbr_offsets, &state.nbrs, &state.height, i);
    let shallow_factor = if h_i <= 0.0 && h_i > params.shallow_sea_floor {
        let shallow_range = (0.0 - params.shallow_sea_floor).max(1e-4);
        let depth = clamp(-h_i, 0.0, shallow_range);
        1.0 - depth / shallow_range
    } else {
        0.0
    };
    let estuary_factor = if h_i > 0.0
        && next_idx.is_some()
        && next_h <= 0.0
    {
        1.0
    } else if h_i <= 0.0 && source_is_coastal && state.sediment[i] > 0.0 {
        0.65
    } else {
        0.0
    };

    let mut water = state.water[i];
    let mut sediment = state.sediment[i];

    if water <= 1e-5 && sediment <= 1e-5 && h_i <= 0.0 {
        state.river_next[i] = next_idx.map(|n| n as i32).unwrap_or(-1);
        state.river_flux[i] *= 0.95;
        result.downstream = next_idx;
        return result;
    }

    let flux_term = water.max(1e-4).powf(0.85);
    let slope_term = local_slope.max(params.erosion_min_slope).powf(0.70);
    let transport_context = clamp(
        1.0 + 0.20 * (1.0 - flattening) - 0.35 * openness - 0.55 * estuary_factor
            + 0.10 * downstream_slope,
        0.15,
        2.5,
    );
    let capacity = params.sediment_capacity_gain * flux_term * slope_term * transport_context;

    if h_i > 0.0 {
        let competence = state
            .params
            .continent_erodibility_from_competence;
        let erodibility = lerp(1.0, 0.5, clamp(competence, 0.0, 1.0));
        let armor_factor = 1.0 - 0.60 * clamp(state.armor[i], 0.0, 1.0);
        let erosion_demand = (capacity - sediment).max(0.0);
        let erode_amount = clamp(
            params.hydraulic_erosion_rate * erosion_demand * erodibility * armor_factor,
            0.0,
            params.erosion_max_delta_per_iter * 0.50,
        );
        if erode_amount > 0.0 {
            state.height[i] = clamp(state.height[i] - erode_amount, -1.2, 1.2);
            sediment += erode_amount;
            result.changed = true;
        }
    }

    let overload = (sediment - capacity).max(0.0);
    if overload > 0.0 {
        let deposit_context = clamp(
            1.0
                + 0.85 * flattening
                + 0.55 * openness
                + 0.85 * estuary_factor
                + 0.75 * shallow_factor
                + if next_idx.is_none() && h_i > 0.0 { 0.8 } else { 0.0 },
            0.3,
            4.0,
        );
        let deposit_cap = if h_i <= 0.0 {
            params.erosion_max_delta_per_iter * 1.25
        } else {
            params.erosion_max_delta_per_iter
        };
        let deposit_amount = clamp(
            params.hydraulic_deposit_rate * overload * deposit_context,
            0.0,
            sediment.min(deposit_cap.max(0.0)),
        );
        if deposit_amount > 0.0 {
            distribute_deposition_direct_by_context(
                &state.nbr_offsets,
                &state.nbrs,
                &mut state.height,
                &mut state.armor,
                params,
                i,
                deposit_amount,
                flattening,
                openness,
                estuary_factor,
                shallow_factor,
            );
            sediment -= deposit_amount;
            result.changed = true;
            result.deposited_here = true;
        }
    }

    if h_i <= params.shallow_sea_floor && sediment > 0.0 {
        let loss = sediment * 0.35;
        sediment = (sediment - loss).max(0.0);
    }

    let mut outflow_water = 0.0f32;
    let mut outflow_sediment = 0.0f32;
    if let Some(n) = next_idx {
        let uphill = state.height[n] > state.height[i] + 1e-5;
        let move_base = if uphill { 0.08 } else { 0.22 };
        let slope_drive = local_slope / (local_slope + 0.02);
        let water_drive = water / (water + 0.05);
        let move_frac = clamp(move_base + 0.55 * slope_drive + 0.20 * water_drive, 0.03, 0.92);
        outflow_water = water * move_frac;
        outflow_sediment = sediment * move_frac;
        state.water[n] += outflow_water;
        state.sediment[n] += outflow_sediment;
        result.downstream = Some(n);
        state.river_next[i] = n as i32;
    } else {
        state.river_next[i] = -1;
    }

    water = (water - outflow_water).max(0.0);
    sediment = (sediment - outflow_sediment).max(0.0);

    let evap = if state.height[i] > 0.0 { 0.30 } else { 0.12 };
    water *= 1.0 - evap;
    if water < 1e-5 {
        water = 0.0;
    }
    if sediment < 1e-6 {
        sediment = 0.0;
    }

    state.water[i] = water;
    state.sediment[i] = sediment;
    state.river_flux[i] = clamp(
        state.river_flux[i] * 0.85 + outflow_water * 0.35 + water * 0.15,
        0.0,
        1.0,
    );

    if (state.height[i] - h_i_before).abs() > 1e-6
        || (state.water[i] - water_before).abs() > 1e-6
        || (state.sediment[i] - sediment_before).abs() > 1e-6
        || state.river_next[i] != river_next_before
        || (state.river_flux[i] - river_flux_before).abs() > 1e-6
    {
        result.changed = true;
    }

    result
}

fn inject_async_rain(
    state: &mut crate::ErosionAutomatonState,
    count: usize,
    changed_mark: &mut [u8],
) {
    let v_count = state.height.len();
    if v_count == 0 || count == 0 {
        return;
    }
    let base = state.params.river_rain_base.max(0.0);
    if base <= 0.0 {
        return;
    }

    for _ in 0..count {
        let v = state.rain_cursor % v_count;
        state.rain_cursor = (state.rain_cursor + 1) % v_count;
        if state.height[v] <= params_shallow_cutoff(&state.params) {
            continue;
        }
        let rain_unit = state.rain[v].max(0.0);
        if rain_unit <= 0.0 {
            continue;
        }
        let add = 0.02 * rain_unit / base.max(1e-4);
        state.water[v] = (state.water[v] + add).min(2.0);
        enqueue_active_vertex(state, v);
        mark_changed_vertex(v, changed_mark, &mut state.recent_changed);
    }
}

fn params_shallow_cutoff(params: &TerrainParams) -> f32 {
    (params.shallow_sea_floor * 0.5).min(0.0)
}

fn pop_active_vertex(state: &mut crate::ErosionAutomatonState) -> Option<usize> {
    while state.active_head < state.active_queue.len() {
        let v = state.active_queue[state.active_head] as usize;
        state.active_head += 1;
        if v >= state.in_queue.len() {
            continue;
        }
        if state.in_queue[v] == 0 {
            continue;
        }
        state.in_queue[v] = 0;
        return Some(v);
    }
    None
}

fn enqueue_active_vertex(state: &mut crate::ErosionAutomatonState, v: usize) {
    if v >= state.in_queue.len() {
        return;
    }
    if state.in_queue[v] != 0 {
        return;
    }
    state.in_queue[v] = 1;
    state.active_queue.push(v as u32);
}

fn enqueue_neighbors(state: &mut crate::ErosionAutomatonState, v: usize) {
    let start = state.nbr_offsets[v] as usize;
    let end = state.nbr_offsets[v + 1] as usize;
    let neighbors = state.nbrs[start..end].to_vec();
    for n_u32 in neighbors {
        enqueue_active_vertex(state, n_u32 as usize);
    }
}

fn compact_active_queue(state: &mut crate::ErosionAutomatonState) {
    if state.active_head == 0 {
        return;
    }
    if state.active_head >= state.active_queue.len() {
        state.active_queue.clear();
        state.active_head = 0;
        return;
    }
    if state.active_head > 4096 || state.active_head * 2 > state.active_queue.len() {
        state.active_queue.drain(0..state.active_head);
        state.active_head = 0;
    }
}

fn mark_changed_vertex(v: usize, changed_mark: &mut [u8], recent_changed: &mut Vec<u32>) {
    if v >= changed_mark.len() {
        return;
    }
    if changed_mark[v] != 0 {
        return;
    }
    changed_mark[v] = 1;
    recent_changed.push(v as u32);
}

fn mark_neighbors_changed(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    v: usize,
    changed_mark: &mut [u8],
    recent_changed: &mut Vec<u32>,
) {
    let start = nbr_offsets[v] as usize;
    let end = nbr_offsets[v + 1] as usize;
    for &n_u32 in &nbrs[start..end] {
        mark_changed_vertex(n_u32 as usize, changed_mark, recent_changed);
    }
}

fn find_local_flow_target(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    v: usize,
) -> (Option<usize>, f32, f32) {
    let start = nbr_offsets[v] as usize;
    let end = nbr_offsets[v + 1] as usize;
    if end <= start {
        return (None, 0.0, -1.0);
    }

    let mut best = None;
    let mut best_h = f32::INFINITY;
    for &n_u32 in &nbrs[start..end] {
        let n = n_u32 as usize;
        let nh = height[n];
        if nh + 1e-6 < best_h {
            best_h = nh;
            best = Some(n);
        }
    }

    if let Some(n) = best {
        let edge_len = chord_distance(positions[v], positions[n]).max(1e-4);
        let slope = ((height[v] - height[n]).max(0.0)) / edge_len;
        (Some(n), slope, height[n])
    } else {
        (None, 0.0, -1.0)
    }
}

fn estimate_local_outflow_slope(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    v: usize,
) -> f32 {
    let start = nbr_offsets[v] as usize;
    let end = nbr_offsets[v + 1] as usize;
    let mut best_slope = 0.0f32;
    for &n_u32 in &nbrs[start..end] {
        let n = n_u32 as usize;
        let edge_len = chord_distance(positions[v], positions[n]).max(1e-4);
        let slope = ((height[v] - height[n]).max(0.0)) / edge_len;
        if slope > best_slope {
            best_slope = slope;
        }
    }
    best_slope
}

fn apply_deposit_direct_to_cell(
    height: &mut [f32],
    armor: &mut [f32],
    params: &TerrainParams,
    v: usize,
    amount: f32,
) {
    if amount <= 0.0 {
        return;
    }
    height[v] = clamp(height[v] + amount, -1.2, 1.2);
    let armor_gain = clamp(amount / params.erosion_max_delta_per_iter.max(1e-6), 0.0, 1.0);
    armor[v] = clamp(armor[v] + 0.55 * armor_gain, 0.0, 1.0);
}

fn distribute_deposition_direct_by_context(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &mut [f32],
    armor: &mut [f32],
    params: &TerrainParams,
    center: usize,
    amount: f32,
    flattening: f32,
    openness: f32,
    estuary_factor: f32,
    shallow_factor: f32,
) {
    if amount <= 0.0 {
        return;
    }

    let spread_strength = clamp(
        0.10 + 0.45 * flattening + 0.35 * openness + 0.35 * estuary_factor + 0.30 * shallow_factor,
        0.0,
        0.92,
    );
    let center_amount = amount * (1.0 - spread_strength);
    apply_deposit_direct_to_cell(height, armor, params, center, center_amount);

    let spread_pool = (amount - center_amount).max(0.0);
    if spread_pool <= 1e-8 {
        return;
    }

    let start = nbr_offsets[center] as usize;
    let end = nbr_offsets[center + 1] as usize;
    if end <= start {
        apply_deposit_direct_to_cell(height, armor, params, center, spread_pool);
        return;
    }

    let center_h = height[center];
    let shallow_range = (0.0 - params.shallow_sea_floor).max(1e-4);
    let mut weights = Vec::with_capacity(end - start);
    let mut weight_sum = 0.0f32;
    for &n_u32 in &nbrs[start..end] {
        let n = n_u32 as usize;
        let nh = height[n];
        let same_band = 1.0 - clamp((nh - center_h).abs() / 0.08, 0.0, 1.0);
        let lower_pref = if nh <= center_h { 1.0 } else { 0.30 };
        let marine_pref = if nh <= 0.0 { 1.0 } else { 0.40 };
        let shallow_pref = if nh <= 0.0 && nh > params.shallow_sea_floor {
            let depth = clamp(-nh, 0.0, shallow_range);
            0.35 + 0.65 * (1.0 - depth / shallow_range)
        } else if nh > 0.0 {
            0.25
        } else {
            0.10
        };
        let w = (0.05 + 0.55 * lower_pref + 0.35 * same_band)
            * (1.0 + 0.70 * openness * same_band)
            * (1.0 + 0.90 * estuary_factor * marine_pref)
            * (1.0 + 1.10 * shallow_factor * shallow_pref);
        if w > 0.0 {
            weight_sum += w;
            weights.push((n, w));
        }
    }

    if weight_sum <= 1e-8 {
        apply_deposit_direct_to_cell(height, armor, params, center, spread_pool);
        return;
    }

    for (n, w) in weights {
        apply_deposit_direct_to_cell(height, armor, params, n, spread_pool * (w / weight_sum));
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
    river_accumulation_threshold: f32,
) -> (Vec<f32>, Vec<i32>) {
    let (mut river_flux, mut river_next) =
        compute_river_flux_and_next(positions, nbr_offsets, nbrs, height, river_rain_base);

    for i in 0..positions.len() {
        if river_flux[i] < river_accumulation_threshold {
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
    let plate_count = {
        let mut unique = std::collections::HashSet::with_capacity(plate_id.len());
        for &pid in &plate_id {
            unique.insert(pid);
        }
        unique.len() as u32
    };
    let land_count = height.iter().filter(|&&h| h > 0.0).count();
    let land_ratio = land_count as f32 / (height.len().max(1) as f32);

    TerrainOutput {
        height,
        plate_id,
        plate_count,
        land_ratio,
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
