use super::*;

pub(in crate::sim::geology) fn apply_hydraulic_erosion(
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
            0.0,
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
                    let mut deposition_buffer = DepositionBuffer {
                        nbr_offsets,
                        nbrs,
                        height,
                        delta: &mut delta,
                        deposition_armor: &mut deposition_armor,
                        params,
                    };
                    distribute_deposition_by_context(
                        &mut deposition_buffer,
                        i,
                        deposit_amount,
                        DepositionContext {
                            flattening,
                            openness,
                            estuary_factor,
                            shallow_factor,
                        },
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
                        let mut deposition_buffer = DepositionBuffer {
                            nbr_offsets,
                            nbrs,
                            height,
                            delta: &mut delta,
                            deposition_armor: &mut deposition_armor,
                            params,
                        };
                        distribute_deposition_by_context(
                            &mut deposition_buffer,
                            i,
                            residual_deep_deposit,
                            DepositionContext {
                                flattening: 0.2,
                                openness: 0.1,
                                estuary_factor: 0.0,
                                shallow_factor: 0.0,
                            },
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

pub(in crate::sim::geology) fn sorted_vertices_by_height_desc(height: &[f32]) -> Vec<usize> {
    let mut order = (0..height.len()).collect::<Vec<_>>();
    order.sort_by(|&a, &b| {
        height[b]
            .partial_cmp(&height[a])
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.cmp(&b))
    });
    order
}

pub(in crate::sim::geology) fn is_coastal_cell(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    height: &[f32],
    v: usize,
) -> bool {
    let h = height[v];
    let start = nbr_offsets[v] as usize;
    let end = nbr_offsets[v + 1] as usize;
    nbrs[start..end].iter().any(|&n_u32| {
        let nh = height[n_u32 as usize];
        (h > 0.0 && nh <= 0.0) || (h <= 0.0 && nh > 0.0)
    })
}

pub(in crate::sim::geology) fn local_open_basin_factor(
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

pub(in crate::sim::geology) fn apply_deposit_to_cell(
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

#[derive(Clone, Copy)]
pub(in crate::sim::geology) struct DepositionContext {
    pub flattening: f32,
    pub openness: f32,
    pub estuary_factor: f32,
    pub shallow_factor: f32,
}

pub(in crate::sim::geology) struct DepositionBuffer<'a> {
    pub nbr_offsets: &'a [u32],
    pub nbrs: &'a [u32],
    pub height: &'a [f32],
    pub delta: &'a mut [f32],
    pub deposition_armor: &'a mut [f32],
    pub params: &'a GeologyParams,
}

pub(in crate::sim::geology) fn distribute_deposition_by_context(
    buffer: &mut DepositionBuffer<'_>,
    center: usize,
    amount: f32,
    context: DepositionContext,
) {
    let nbr_offsets = buffer.nbr_offsets;
    let nbrs = buffer.nbrs;
    let height = buffer.height;
    let delta = &mut *buffer.delta;
    let deposition_armor = &mut *buffer.deposition_armor;
    let params = buffer.params;
    let flattening = context.flattening;
    let openness = context.openness;
    let estuary_factor = context.estuary_factor;
    let shallow_factor = context.shallow_factor;

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
