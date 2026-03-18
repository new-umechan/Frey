use super::*;

pub(super) fn postprocess_height(
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
        let buoyancy_bias = if attributes[pid].is_ocean {
            -0.09
        } else {
            0.09
        };
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

pub(super) fn apply_hotspot_island_chains(
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

pub(super) fn apply_hydraulic_erosion(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    vertex_competence: &[f32],
    height: &mut [f32],
    params: &GeologyParams,
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
            let capacity =
                params.sediment_capacity_gain * flux_term * slope_term * transport_context;

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
                    1.0 + 0.85 * flattening
                        + 0.55 * openness
                        + 0.85 * estuary_factor
                        + 0.75 * shallow_factor
                        + if next_idx.is_none() && h_i > 0.0 {
                            0.8
                        } else {
                            0.0
                        },
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
                    let residual_deep_deposit = clamp(
                        sediment * 0.20,
                        0.0,
                        params.erosion_max_delta_per_iter * 0.75,
                    );
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

pub(super) fn sorted_vertices_by_height_desc(height: &[f32]) -> Vec<usize> {
    let mut order = (0..height.len()).collect::<Vec<_>>();
    order.sort_by(|&a, &b| {
        height[b]
            .partial_cmp(&height[a])
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.cmp(&b))
    });
    order
}

pub(super) fn is_coastal_cell(nbr_offsets: &[u32], nbrs: &[u32], height: &[f32], v: usize) -> bool {
    let h = height[v];
    let start = nbr_offsets[v] as usize;
    let end = nbr_offsets[v + 1] as usize;
    nbrs[start..end].iter().any(|&n_u32| {
        let nh = height[n_u32 as usize];
        (h > 0.0 && nh <= 0.0) || (h <= 0.0 && nh > 0.0)
    })
}

pub(super) fn local_open_basin_factor(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    v: usize,
) -> f32 {
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

pub(super) fn apply_deposit_to_cell(
    delta: &mut [f32],
    deposition_armor: &mut [f32],
    params: &GeologyParams,
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

pub(super) fn distribute_deposition_by_context(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    delta: &mut [f32],
    deposition_armor: &mut [f32],
    params: &GeologyParams,
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

    apply_deposit_to_cell(
        delta,
        deposition_armor,
        params,
        center,
        center_amount.min(amount),
    );

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
        apply_deposit_to_cell(
            delta,
            deposition_armor,
            params,
            m,
            spread_pool * (w / weight_sum),
        );
    }
}

#[cfg(target_arch = "wasm32")]
type ProfileClock = f64;
#[cfg(not(target_arch = "wasm32"))]
type ProfileClock = std::time::Instant;

#[cfg(target_arch = "wasm32")]
pub(super) fn profile_now() -> ProfileClock {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn profile_now() -> ProfileClock {
    std::time::Instant::now()
}

#[cfg(target_arch = "wasm32")]
pub(super) fn profile_elapsed_ms(start: ProfileClock) -> f64 {
    js_sys::Date::now() - start
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn profile_elapsed_ms(start: ProfileClock) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

const FULL_REBUILD_INTERVAL_TICKS: u64 = 8;
const FULL_REBUILD_CHANGED_RATIO: f32 = 0.02;

pub(super) fn sink_buffers_ready(state: &crate::ErosionAutomatonState, v_count: usize) -> bool {
    state.sink_id.len() == v_count
        && state.sink_route_next.len() == v_count
        && state.sink_dirty.len() == v_count
        && state.sink_spill_cell.len() == state.sink_spill_to.len()
        && state.sink_spill_cell.len() == state.sink_spill_level.len()
        && state.sink_spill_cell.len() == state.sink_capacity_total.len()
        && state.sink_spill_cell.len() == state.sink_capacity_remaining.len()
        && state.sink_spill_cell.len() == state.sink_storage_sediment.len()
        && state.sink_spill_cell.len() == state.sink_overflow_active.len()
}

pub(crate) fn step_async_erosion_automaton(
    state: &mut crate::ErosionAutomatonState,
    budget_cells: u32,
) -> crate::sim::ErosionAutomatonBreakdown {
    let mut breakdown = crate::sim::ErosionAutomatonBreakdown::default();
    let v_count = state.height.len();
    if v_count == 0 {
        return breakdown;
    }
    if state.prev_river_next.len() != v_count {
        state.prev_river_next = state.river_next.clone();
    }
    if state.flow_heading.len() != v_count {
        state.flow_heading = vec![[0.0, 0.0, 0.0]; v_count];
    }
    if state.groundwater_storage.len() != v_count {
        state.groundwater_storage = vec![0.0; v_count];
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
        return breakdown;
    }

    let budget = budget_cells.max(1) as usize;
    state.tick = state.tick.saturating_add(1);

    let mut previous_changed = std::mem::take(&mut state.recent_changed);
    let phase_start = profile_now();
    let changed_ratio = if v_count > 0 {
        previous_changed.len() as f32 / v_count as f32
    } else {
        0.0
    };
    let force_full_rebuild = state.tick <= 1
        || !sink_buffers_ready(state, v_count)
        || changed_ratio >= FULL_REBUILD_CHANGED_RATIO
        || state.tick.saturating_sub(state.last_sink_full_rebuild_tick)
            >= FULL_REBUILD_INTERVAL_TICKS;
    let sink_rebuild_stats = rebuild_sink_state(state, &previous_changed, force_full_rebuild);
    breakdown.sink_rebuild_ms += profile_elapsed_ms(phase_start);
    breakdown.sink_rebuild_full_count = sink_rebuild_stats.full_count;
    breakdown.sink_rebuild_partial_count = sink_rebuild_stats.partial_count;
    breakdown.sink_rebuild_skipped_count = sink_rebuild_stats.skipped_count;
    breakdown.sink_rebuild_fallback_full_count = sink_rebuild_stats.fallback_full_count;
    previous_changed.clear();
    state.recent_changed = previous_changed;

    let mut changed_mark = std::mem::take(&mut state.scratch_changed_mark);
    if changed_mark.len() != v_count {
        changed_mark = vec![0; v_count];
    } else {
        changed_mark.fill(0);
    }
    let rain_inject_count = ((budget / 2).clamp(16, 256)).min(v_count);
    inject_async_rain(state, rain_inject_count, &mut changed_mark);

    let mut processed = 0usize;
    let cell_phase_start = profile_now();
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
    breakdown.cell_process_ms += profile_elapsed_ms(cell_phase_start);

    let queue_phase_start = profile_now();
    compact_active_queue(state);
    breakdown.queue_update_ms += profile_elapsed_ms(queue_phase_start);
    state.scratch_changed_mark = changed_mark;
    breakdown
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

    state.armor[i] *= 0.985;

    let (mut next_idx, local_slope, next_h) = find_local_flow_target(
        &state.positions,
        &state.nbr_offsets,
        &state.nbrs,
        &state.height,
        &state.river_next,
        &state.flow_heading,
        &state.params,
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
    let estuary_factor = if h_i > 0.0 && next_idx.is_some() && next_h <= 0.0 {
        1.0
    } else if h_i <= 0.0 && source_is_coastal && state.sediment[i] > 0.0 {
        0.65
    } else {
        0.0
    };

    let mut water = state.water[i];
    let mut sediment = state.sediment[i];

    if water <= 1e-5 && sediment <= 1e-5 && h_i <= 0.0 {
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
        let competence = state.params.continent_erodibility_from_competence;
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
            1.0 + 0.85 * flattening
                + 0.55 * openness
                + 0.85 * estuary_factor
                + 0.75 * shallow_factor
                + if next_idx.is_none() && h_i > 0.0 {
                    0.8
                } else {
                    0.0
                },
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

    let sink_before_next = next_idx;
    apply_sink_capacity_rule(state, i, &mut sediment, &mut next_idx);
    if next_idx != sink_before_next {
        result.changed = true;
    }

    let mut outflow_water = 0.0f32;
    let mut outflow_sediment = 0.0f32;
    if let Some(n) = next_idx {
        let uphill = state.height[n] > state.height[i] + 1e-5;
        let move_base = if uphill { 0.08 } else { 0.22 };
        let slope_drive = local_slope / (local_slope + 0.02);
        let water_drive = water / (water + 0.05);
        let move_frac = clamp(
            move_base + 0.55 * slope_drive + 0.20 * water_drive,
            0.03,
            0.92,
        );
        outflow_water = water * move_frac;
        outflow_sediment = sediment * move_frac;
        state.water[n] += outflow_water;
        state.sediment[n] += outflow_sediment;
        result.downstream = Some(n);
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

    if (state.height[i] - h_i_before).abs() > 1e-6
        || (state.water[i] - water_before).abs() > 1e-6
        || (state.sediment[i] - sediment_before).abs() > 1e-6
    {
        result.changed = true;
    }

    result
}
pub(super) fn inject_async_rain(
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

pub(super) fn params_shallow_cutoff(params: &GeologyParams) -> f32 {
    (params.shallow_sea_floor * 0.5).min(0.0)
}

pub(super) fn pop_active_vertex(state: &mut crate::ErosionAutomatonState) -> Option<usize> {
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

pub(super) fn enqueue_active_vertex(state: &mut crate::ErosionAutomatonState, v: usize) {
    if v >= state.in_queue.len() {
        return;
    }
    if state.in_queue[v] != 0 {
        return;
    }
    state.in_queue[v] = 1;
    state.active_queue.push(v as u32);
}

pub(super) fn enqueue_neighbors(state: &mut crate::ErosionAutomatonState, v: usize) {
    let start = state.nbr_offsets[v] as usize;
    let end = state.nbr_offsets[v + 1] as usize;
    for idx in start..end {
        let n = state.nbrs[idx] as usize;
        enqueue_active_vertex(state, n);
    }
}

pub(super) fn compact_active_queue(state: &mut crate::ErosionAutomatonState) {
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

pub(super) fn mark_changed_vertex(
    v: usize,
    changed_mark: &mut [u8],
    recent_changed: &mut Vec<u32>,
) {
    if v >= changed_mark.len() {
        return;
    }
    if changed_mark[v] != 0 {
        return;
    }
    changed_mark[v] = 1;
    recent_changed.push(v as u32);
}

pub(super) fn mark_neighbors_changed(
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

pub(super) fn find_local_flow_target(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    prev_next: &[i32],
    flow_heading: &[[f32; 3]],
    params: &GeologyParams,
    v: usize,
) -> (Option<usize>, f32, f32) {
    let start = nbr_offsets[v] as usize;
    let end = nbr_offsets[v + 1] as usize;
    if end <= start {
        return (None, 0.0, -1.0);
    }

    let mut best = None;
    let mut best_score = f32::NEG_INFINITY;
    let mut best_h = height[v];
    for &n_u32 in &nbrs[start..end] {
        let n = n_u32 as usize;
        let nh = height[n];
        if nh + 1e-6 >= height[v] {
            continue;
        }

        let edge_len = chord_distance(positions[v], positions[n]).max(1e-4);
        let drop = (height[v] - nh).max(0.0);
        let mut score = drop / edge_len;

        if prev_next.get(v).copied().unwrap_or(-1) == n as i32 {
            score += params.river_inertia_gain * 0.4;
        }
        let prev_dir = flow_heading.get(v).copied().unwrap_or([0.0, 0.0, 0.0]);
        let prev_len =
            (prev_dir[0] * prev_dir[0] + prev_dir[1] * prev_dir[1] + prev_dir[2] * prev_dir[2])
                .sqrt();
        if prev_len > 1e-6 {
            let cand_dir = normalize3([
                positions[n][0] - positions[v][0],
                positions[n][1] - positions[v][1],
                positions[n][2] - positions[v][2],
            ]);
            let align = clamp(
                prev_dir[0] * cand_dir[0] + prev_dir[1] * cand_dir[1] + prev_dir[2] * cand_dir[2],
                -1.0,
                1.0,
            );
            score += params.river_inertia_gain * 0.2 * align.max(0.0);
            score -= params.river_curvature_penalty * 0.5 * (1.0 - align).max(0.0);
        }

        if score > best_score {
            best_score = score;
            best_h = nh;
            best = Some(n);
        }
    }

    if let Some(n) = best {
        let edge_len = chord_distance(positions[v], positions[n]).max(1e-4);
        let slope = ((height[v] - height[n]).max(0.0)) / edge_len;
        (Some(n), slope, best_h)
    } else {
        (None, 0.0, -1.0)
    }
}

pub(super) fn estimate_local_outflow_slope(
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

pub(super) fn apply_deposit_direct_to_cell(
    height: &mut [f32],
    armor: &mut [f32],
    params: &GeologyParams,
    v: usize,
    amount: f32,
) {
    if amount <= 0.0 {
        return;
    }
    height[v] = clamp(height[v] + amount, -1.2, 1.2);
    let armor_gain = clamp(
        amount / params.erosion_max_delta_per_iter.max(1e-6),
        0.0,
        1.0,
    );
    armor[v] = clamp(armor[v] + 0.55 * armor_gain, 0.0, 1.0);
}

pub(super) fn distribute_deposition_direct_by_context(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &mut [f32],
    armor: &mut [f32],
    params: &GeologyParams,
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

const PARTIAL_INVALID_SPILL_RATIO: f32 = 0.20;

#[derive(Clone, Copy, Debug, Default)]
struct SinkRebuildStats {
    full_count: u32,
    partial_count: u32,
    skipped_count: u32,
    fallback_full_count: u32,
}

fn rebuild_sink_state(
    state: &mut crate::ErosionAutomatonState,
    changed: &[u32],
    force_full: bool,
) -> SinkRebuildStats {
    if force_full {
        rebuild_sink_state_full(state);
        state.last_sink_full_rebuild_tick = state.tick;
        return SinkRebuildStats {
            full_count: 1,
            ..SinkRebuildStats::default()
        };
    }
    if changed.is_empty() {
        return SinkRebuildStats {
            skipped_count: 1,
            ..SinkRebuildStats::default()
        };
    }
    let (affected_count, invalid_count) = rebuild_sink_state_partial(state, changed);
    if affected_count == 0 {
        return SinkRebuildStats {
            skipped_count: 1,
            ..SinkRebuildStats::default()
        };
    }
    let invalid_ratio = invalid_count as f32 / affected_count as f32;
    if invalid_ratio > PARTIAL_INVALID_SPILL_RATIO {
        rebuild_sink_state_full(state);
        state.last_sink_full_rebuild_tick = state.tick;
        return SinkRebuildStats {
            full_count: 1,
            partial_count: 1,
            fallback_full_count: 1,
            ..SinkRebuildStats::default()
        };
    }
    SinkRebuildStats {
        partial_count: 1,
        ..SinkRebuildStats::default()
    }
}

pub(super) fn rebuild_sink_state_full(state: &mut crate::ErosionAutomatonState) {
    let v_count = state.height.len();
    if v_count == 0 {
        return;
    }
    reset_sink_buffers(state, v_count);

    let downhill = compute_downhill_links(&state.height, &state.nbr_offsets, &state.nbrs);

    let mut terminal = vec![-2_i32; v_count];
    for i in 0..v_count {
        terminal[i] = trace_terminal(i, &state.height, &downhill, &mut terminal);
    }

    let sink_members = build_sink_members(&state.height, &terminal, &mut state.sink_id);
    let old_state = snapshot_sink_state(state);
    let sink_count = sink_members.len();
    resize_sink_state_arrays(state, sink_count);

    for (sid, members) in sink_members.iter().enumerate() {
        update_sink_for_sid(state, sid, members, &old_state);
    }
}

pub(super) fn rebuild_sink_state_partial(
    state: &mut crate::ErosionAutomatonState,
    changed: &[u32],
) -> (usize, usize) {
    let v_count = state.height.len();
    if v_count == 0 {
        return (0, 0);
    }
    let sink_count = state.sink_spill_cell.len();
    if sink_count == 0 {
        return (0, 0);
    }
    let mut affected_mark = vec![0u8; v_count];
    for &v_u32 in changed {
        let v = v_u32 as usize;
        if v >= v_count {
            continue;
        }
        affected_mark[v] = 1;
        let start = state.nbr_offsets[v] as usize;
        let end = state.nbr_offsets[v + 1] as usize;
        for &n_u32 in &state.nbrs[start..end] {
            let n = n_u32 as usize;
            if n < v_count {
                affected_mark[n] = 1;
            }
        }
    }

    let mut affected_sink_ids = Vec::<usize>::new();
    let mut sink_seen = vec![0u8; sink_count];
    for (cell, mark) in affected_mark.iter().enumerate() {
        if *mark == 0 {
            continue;
        }
        let sid_raw = state.sink_id[cell];
        if sid_raw < 0 {
            continue;
        }
        let sid = sid_raw as usize;
        if sid >= sink_count || sink_seen[sid] != 0 {
            continue;
        }
        sink_seen[sid] = 1;
        affected_sink_ids.push(sid);
    }
    if affected_sink_ids.is_empty() {
        return (0, 0);
    }
    affected_sink_ids.sort_unstable();

    let old_state = snapshot_sink_state(state);
    let mut member_map = std::collections::HashMap::<usize, Vec<usize>>::new();
    for (cell, sid_raw) in state.sink_id.iter().copied().enumerate() {
        if sid_raw < 0 {
            continue;
        }
        let sid = sid_raw as usize;
        if sid >= sink_count || sink_seen[sid] == 0 {
            continue;
        }
        member_map.entry(sid).or_default().push(cell);
    }

    let mut invalid_count = 0usize;
    for sid in &affected_sink_ids {
        let members = member_map.get(sid).map(|v| v.as_slice()).unwrap_or(&[]);
        update_sink_for_sid(state, *sid, members, &old_state);
        if state.sink_spill_cell.get(*sid).copied().unwrap_or(-1) < 0 {
            invalid_count += 1;
        }
    }
    (affected_sink_ids.len(), invalid_count)
}

pub(super) fn update_sink_for_sid(
    state: &mut crate::ErosionAutomatonState,
    sid: usize,
    members: &[usize],
    old_state: &std::collections::HashMap<(i32, i32), (f32, f32, u8)>,
) {
    if sid >= state.sink_spill_cell.len() {
        return;
    }
    if members.is_empty() {
        state.sink_spill_cell[sid] = -1;
        state.sink_spill_to[sid] = -1;
        state.sink_spill_level[sid] = 0.0;
        state.sink_capacity_total[sid] = 0.0;
        state.sink_capacity_remaining[sid] = 0.0;
        state.sink_storage_sediment[sid] = 0.0;
        state.sink_overflow_active[sid] = 1;
        return;
    }
    let (best_level, best_from, best_to) = find_sink_spill_edge(
        &state.height,
        &state.nbr_offsets,
        &state.nbrs,
        &state.sink_id,
        sid,
        members,
    );

    state.sink_spill_cell[sid] = best_from;
    state.sink_spill_to[sid] = best_to;
    state.sink_spill_level[sid] = best_level;
    if best_from < 0 {
        state.sink_capacity_total[sid] = 0.0;
        state.sink_capacity_remaining[sid] = 0.0;
        state.sink_storage_sediment[sid] = 0.0;
        state.sink_overflow_active[sid] = 1;
        for &v in members {
            state.sink_route_next[v] = -1;
        }
        return;
    }

    let cap = sink_capacity(
        &state.height,
        members,
        best_level,
        state.params.sink_min_capacity,
    );
    state.sink_capacity_total[sid] = cap;
    restore_sink_snapshot(state, sid, cap, best_from, best_to, old_state);
    rebuild_sink_route_for_sid(state, sid, members);
}

pub(super) fn reset_sink_buffers(state: &mut crate::ErosionAutomatonState, v_count: usize) {
    if state.sink_id.len() != v_count {
        state.sink_id = vec![-1; v_count];
    } else {
        state.sink_id.fill(-1);
    }
    if state.sink_route_next.len() != v_count {
        state.sink_route_next = vec![-1; v_count];
    } else {
        state.sink_route_next.fill(-1);
    }
    if state.sink_dirty.len() != v_count {
        state.sink_dirty = vec![0; v_count];
    } else {
        state.sink_dirty.fill(0);
    }
}

pub(super) fn compute_downhill_links(
    height: &[f32],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> Vec<i32> {
    let mut downhill = vec![-1_i32; height.len()];
    for i in 0..height.len() {
        if height[i] <= 0.0 {
            continue;
        }
        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        let mut best = -1_i32;
        let mut best_h = height[i];
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            let nh = height[n];
            if nh + 1e-6 < best_h {
                best_h = nh;
                best = n as i32;
            }
        }
        downhill[i] = best;
    }
    downhill
}

pub(super) fn build_sink_members(
    height: &[f32],
    terminal: &[i32],
    sink_id: &mut [i32],
) -> Vec<Vec<usize>> {
    let mut root_to_sink = std::collections::HashMap::<usize, usize>::new();
    let mut sink_members = Vec::<Vec<usize>>::new();
    for i in 0..terminal.len() {
        let root = terminal[i];
        if root < 0 {
            continue;
        }
        let r = root as usize;
        if height[r] <= 0.0 {
            continue;
        }
        let sid = *root_to_sink.entry(r).or_insert_with(|| {
            sink_members.push(Vec::new());
            sink_members.len() - 1
        });
        sink_id[i] = sid as i32;
        sink_members[sid].push(i);
    }
    sink_members
}

pub(super) fn snapshot_sink_state(
    state: &crate::ErosionAutomatonState,
) -> std::collections::HashMap<(i32, i32), (f32, f32, u8)> {
    let mut old_state = std::collections::HashMap::<(i32, i32), (f32, f32, u8)>::new();
    for sid in 0..state.sink_spill_cell.len() {
        let spill_cell = state.sink_spill_cell[sid];
        let spill_to = state.sink_spill_to.get(sid).copied().unwrap_or(-1);
        let remain = state
            .sink_capacity_remaining
            .get(sid)
            .copied()
            .unwrap_or(0.0);
        let storage = state.sink_storage_sediment.get(sid).copied().unwrap_or(0.0);
        let active = state.sink_overflow_active.get(sid).copied().unwrap_or(0);
        old_state.insert((spill_cell, spill_to), (remain, storage, active));
    }
    old_state
}

pub(super) fn resize_sink_state_arrays(
    state: &mut crate::ErosionAutomatonState,
    sink_count: usize,
) {
    state.sink_spill_cell = vec![-1; sink_count];
    state.sink_spill_to = vec![-1; sink_count];
    state.sink_spill_level = vec![0.0; sink_count];
    state.sink_capacity_total = vec![0.0; sink_count];
    state.sink_capacity_remaining = vec![0.0; sink_count];
    state.sink_storage_sediment = vec![0.0; sink_count];
    state.sink_overflow_active = vec![0; sink_count];
}

pub(super) fn find_sink_spill_edge(
    height: &[f32],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    sink_id: &[i32],
    sid: usize,
    members: &[usize],
) -> (f32, i32, i32) {
    let mut best_level = f32::INFINITY;
    let mut best_from = -1_i32;
    let mut best_to = -1_i32;
    for &v in members {
        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if sink_id[n] == sid as i32 {
                continue;
            }
            let cand_level = height[v].max(height[n]);
            if cand_level + 1e-6 < best_level {
                best_level = cand_level;
                best_from = v as i32;
                best_to = n as i32;
            }
        }
    }
    (best_level, best_from, best_to)
}

pub(super) fn sink_capacity(
    height: &[f32],
    members: &[usize],
    spill_level: f32,
    sink_min_capacity: f32,
) -> f32 {
    let mut capacity = 0.0f32;
    for &v in members {
        capacity += (spill_level - height[v]).max(0.0);
    }
    capacity.max(sink_min_capacity.max(0.0))
}

pub(super) fn restore_sink_snapshot(
    state: &mut crate::ErosionAutomatonState,
    sid: usize,
    cap: f32,
    best_from: i32,
    best_to: i32,
    old_state: &std::collections::HashMap<(i32, i32), (f32, f32, u8)>,
) {
    if let Some((old_remain, old_storage, old_active)) = old_state.get(&(best_from, best_to)) {
        state.sink_capacity_remaining[sid] = old_remain.clamp(0.0, cap);
        state.sink_storage_sediment[sid] = old_storage.max(0.0);
        state.sink_overflow_active[sid] = *old_active;
    } else {
        state.sink_capacity_remaining[sid] = cap;
        state.sink_storage_sediment[sid] = 0.0;
        state.sink_overflow_active[sid] = 0;
    }
}

pub(super) fn trace_terminal(
    i: usize,
    height: &[f32],
    downhill: &[i32],
    terminal: &mut [i32],
) -> i32 {
    if i >= downhill.len() {
        return -1;
    }
    if height[i] <= 0.0 {
        terminal[i] = -1;
        return -1;
    }
    if terminal[i] >= -1 {
        return terminal[i];
    }
    terminal[i] = -3;
    let next = downhill[i];
    let out = if next < 0 {
        i as i32
    } else {
        let n = next as usize;
        if n >= downhill.len() {
            -1
        } else {
            let traced = trace_terminal(n, height, downhill, terminal);
            if traced == -3 {
                i as i32
            } else {
                traced
            }
        }
    };
    terminal[i] = out;
    out
}

pub(super) fn rebuild_sink_route_for_sid(
    state: &mut crate::ErosionAutomatonState,
    sid: usize,
    members: &[usize],
) {
    let spill_from = state.sink_spill_cell[sid];
    if spill_from < 0 {
        return;
    }
    let source = spill_from as usize;
    let mut is_member = vec![0u8; state.height.len()];
    for &v in members {
        is_member[v] = 1;
        state.sink_route_next[v] = -1;
    }

    let mut dist = vec![f32::INFINITY; state.height.len()];
    let mut heap = BinaryHeap::<FlowRouteState>::new();
    dist[source] = 0.0;
    heap.push(FlowRouteState {
        vertex: source,
        spill_level: 0.0,
        steps: 0,
    });

    while let Some(cur) = heap.pop() {
        let v = cur.vertex;
        let d = -cur.spill_level;
        if d > dist[v] + 1e-6 {
            continue;
        }
        let start = state.nbr_offsets[v] as usize;
        let end = state.nbr_offsets[v + 1] as usize;
        for &n_u32 in &state.nbrs[start..end] {
            let n = n_u32 as usize;
            if is_member[n] == 0 {
                continue;
            }
            let uphill = (state.height[n] - state.height[v]).max(0.0);
            let cand = d + 1.0 + uphill * 8.0;
            if cand + 1e-6 < dist[n] {
                dist[n] = cand;
                heap.push(FlowRouteState {
                    vertex: n,
                    spill_level: -cand,
                    steps: cur.steps.saturating_add(1),
                });
            }
        }
    }

    for &v in members {
        if v == source || !dist[v].is_finite() {
            continue;
        }
        let start = state.nbr_offsets[v] as usize;
        let end = state.nbr_offsets[v + 1] as usize;
        let mut best = -1_i32;
        let mut best_dist = dist[v];
        for &n_u32 in &state.nbrs[start..end] {
            let n = n_u32 as usize;
            if is_member[n] == 0 {
                continue;
            }
            if dist[n] + 1e-6 < best_dist {
                best_dist = dist[n];
                best = n as i32;
            }
        }
        state.sink_route_next[v] = best;
    }
}

pub(super) fn apply_sink_capacity_rule(
    state: &mut crate::ErosionAutomatonState,
    cell: usize,
    sediment: &mut f32,
    next_idx: &mut Option<usize>,
) {
    if cell >= state.sink_id.len() {
        return;
    }
    let sid_raw = state.sink_id[cell];
    if sid_raw < 0 {
        return;
    }
    let sid = sid_raw as usize;
    if sid >= state.sink_capacity_remaining.len()
        || sid >= state.sink_overflow_active.len()
        || sid >= state.sink_spill_cell.len()
    {
        return;
    }
    let spill_level = state
        .sink_spill_level
        .get(sid)
        .copied()
        .unwrap_or(f32::INFINITY);
    let hysteresis = state.params.sink_overflow_hysteresis.max(0.0);
    let is_pond_cell = state.height[cell] <= spill_level + hysteresis;
    if !is_pond_cell {
        return;
    }

    if state.sink_overflow_active[sid] == 0 {
        if let Some(next) = *next_idx {
            if next < state.sink_id.len() && state.sink_id[next] != sid_raw {
                *next_idx = None;
            }
        }

        let remain = state.sink_capacity_remaining[sid];
        if remain > hysteresis && *sediment > 0.0 {
            let capture = (*sediment * state.params.hydraulic_deposit_rate.max(0.0))
                .max((*sediment * 0.10).min(*sediment))
                .min(remain)
                .min(*sediment);
            if capture > 0.0 {
                apply_deposit_direct_to_cell(
                    &mut state.height,
                    &mut state.armor,
                    &state.params,
                    cell,
                    capture,
                );
                *sediment = (*sediment - capture).max(0.0);
                state.sink_capacity_remaining[sid] = (remain - capture).max(0.0);
                state.sink_storage_sediment[sid] += capture;
            }
        }

        if state.sink_capacity_remaining[sid] <= hysteresis {
            state.sink_overflow_active[sid] = 1;
            enqueue_sink_local_area(state, sid);
        }
        return;
    }

    let spill_from = state.sink_spill_cell[sid];
    let spill_to = state.sink_spill_to.get(sid).copied().unwrap_or(-1);
    if spill_from >= 0 && spill_to >= 0 && cell == spill_from as usize {
        *next_idx = Some(spill_to as usize);
        return;
    }

    let route = state.sink_route_next.get(cell).copied().unwrap_or(-1);
    if route >= 0 {
        *next_idx = Some(route as usize);
    }
}

pub(super) fn enqueue_sink_local_area(state: &mut crate::ErosionAutomatonState, sid: usize) {
    let spill = state.sink_spill_cell.get(sid).copied().unwrap_or(-1);
    if spill < 0 {
        return;
    }
    let start = spill as usize;
    let max_depth = state.params.sink_local_rebuild_radius.max(1) as usize;
    let mut seen = vec![0u8; state.height.len()];
    let mut queue = std::collections::VecDeque::<(usize, usize)>::new();
    queue.push_back((start, 0));
    seen[start] = 1;
    while let Some((v, depth)) = queue.pop_front() {
        enqueue_active_vertex(state, v);
        if depth >= max_depth {
            continue;
        }
        let s = state.nbr_offsets[v] as usize;
        let e = state.nbr_offsets[v + 1] as usize;
        for &n_u32 in &state.nbrs[s..e] {
            let n = n_u32 as usize;
            if seen[n] != 0 {
                continue;
            }
            seen[n] = 1;
            queue.push_back((n, depth + 1));
        }
    }
}

pub(super) fn compute_river_flux_and_next(
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

pub(super) fn flow_key_better(level_a: f32, steps_a: u32, level_b: f32, steps_b: u32) -> bool {
    level_a + 1e-6 < level_b || ((level_a - level_b).abs() <= 1e-6 && steps_a < steps_b)
}

pub(super) fn flow_key_strictly_decreases(
    from_level: f32,
    from_steps: u32,
    to_level: f32,
    to_steps: u32,
) -> bool {
    flow_key_better(to_level, to_steps, from_level, from_steps)
}

pub(super) fn compute_overflow_route_keys(
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

pub(super) fn compute_lake_depth_map(
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

pub(super) fn generate_rivers(
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

pub(super) fn build_precipitation_map(
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

pub(super) fn prevailing_wind_dir(p: [f32; 3], lat: f32) -> [f32; 3] {
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

pub(super) fn directional_neighbor_heights(
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

pub(super) fn earth_preset(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    river_rain_base: f32,
) -> GeologyOutput {
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

    GeologyOutput {
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
        vertex_age_norm: vec![0.4, 0.0, 0.0, 0.6],
        vertex_buoyancy: vec![-0.08, 0.14, 0.08, -0.10],
        debug_trench_strength: vec![0.0; positions.len()],
        debug_arc_strength: vec![0.0; positions.len()],
        debug_backarc_strength: vec![0.0; positions.len()],
        debug_ocean_ocean_arc_strength: vec![0.0; positions.len()],
    }
}
