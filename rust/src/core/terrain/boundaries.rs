fn apply_boundary_model(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    attributes: &[PlateAttr],
    vertex_lithosphere: &[VertexLithosphere],
    boundary_edges: &[BoundaryEdge],
    height: &mut [f32],
    params: &TerrainParams,
) -> BoundaryFields {
    if boundary_edges.is_empty() {
        return BoundaryFields {
            preserve_strength: vec![0.0; height.len()],
            debug_trench_strength: vec![0.0; height.len()],
            debug_arc_strength: vec![0.0; height.len()],
            debug_backarc_strength: vec![0.0; height.len()],
            debug_ocean_ocean_arc_strength: vec![0.0; height.len()],
        };
    }

    let (nearest_edge, boundary_dist, boundary_vertices) =
        compute_boundary_distance_assignment(positions, nbr_offsets, nbrs, &boundary_edges, height.len());

    let mut delta = vec![0.0_f32; height.len()];
    let mut preserve_strength = vec![0.0_f32; height.len()];
    let mut debug_trench_strength = vec![0.0_f32; height.len()];
    let mut debug_arc_strength = vec![0.0_f32; height.len()];
    let mut debug_backarc_strength = vec![0.0_f32; height.len()];
    let mut debug_ocean_ocean_arc_strength = vec![0.0_f32; height.len()];

    for v in 0..height.len() {
        let edge_idx = nearest_edge[v];
        if edge_idx == usize::MAX {
            continue;
        }
        let edge = boundary_edges[edge_idx];
        let pid = plate_id[v] as usize;
        let d = boundary_dist[v];
        let dist_scale = (-(d * params.boundary_distance_falloff)).exp();

        match edge.boundary_type {
            BoundaryType::Convergent => {
                let oblique_relief = 1.0 - params.boundary_obliquity_mix * edge.obliquity;
                let conv_base = params.boundary_convergent_base_gain * edge.strength * oblique_relief;

                let (convergent_mode, subduction_polarity) = classify_convergent_edge(
                    edge.a,
                    edge.b,
                    edge.plate_a,
                    edge.plate_b,
                    attributes,
                    vertex_lithosphere,
                );
                if let Some(mode) = convergent_mode {
                    match mode {
                        ConvergentMode::ContinentContinent => {
                            let w = band_weight(d, params.boundary_width_collision, params.boundary_anisotropy);
                            let uplift = conv_base * params.collision_gain * w;
                            delta[v] += uplift;
                            if d < params.boundary_width_collision * 0.55 {
                                delta[v] -= 0.10 * uplift;
                            }
                            preserve_strength[v] = preserve_strength[v].max(0.80 * w);
                        }
                        ConvergentMode::OceanContinent | ConvergentMode::OceanOcean => {
                            let (subducting, overriding) = match subduction_polarity {
                                SubductionPolarity::AUnderB => (edge.plate_a, edge.plate_b),
                                SubductionPolarity::BUnderA => (edge.plate_b, edge.plate_a),
                                SubductionPolarity::None => (usize::MAX, usize::MAX),
                            };
                            let subduction_angle = estimate_subduction_angle_proxy(
                                edge,
                                subduction_polarity,
                                attributes,
                                vertex_lithosphere,
                            );
                            let trench_depth_scale = lerp(0.90, 1.28, subduction_angle);
                            let trench_width_scale = lerp(1.16, 0.88, subduction_angle);
                            let arc_offset_scale = lerp(1.22, 0.82, subduction_angle);
                            let forearc_offset_scale = lerp(0.70, 0.48, subduction_angle);
                            let forearc_width_scale = lerp(0.95, 0.72, subduction_angle);
                            let backarc_offset_scale = lerp(1.65, 0.95, subduction_angle);
                            let backarc_width_scale = lerp(1.15, 0.88, subduction_angle);

                            if pid == subducting {
                                let trench_w = band_weight(
                                    d,
                                    params.boundary_width_trench
                                        * trench_width_scale
                                        * (0.9 + 0.35 * edge.obliquity),
                                    params.boundary_anisotropy,
                                );
                                let trench =
                                    conv_base * params.trench_gain * trench_depth_scale * trench_w;
                                delta[v] -= trench;
                                debug_trench_strength[v] =
                                    debug_trench_strength[v].max((edge.strength * trench_w).min(1.0));
                                let outer_rise = ring_weight(
                                    d,
                                    params.boundary_width_trench * trench_width_scale * 1.6,
                                    params.boundary_width_trench * 0.65,
                                );
                                delta[v] += 0.12 * conv_base * outer_rise * dist_scale;
                                preserve_strength[v] = preserve_strength[v].max(0.95 * trench_w);
                            } else if pid == overriding {
                                let forearc_center = params.boundary_width_trench
                                    * trench_width_scale
                                    * forearc_offset_scale
                                    * (1.0 + 0.15 * edge.obliquity);
                                let forearc_w = ring_weight(
                                    d,
                                    forearc_center,
                                    params.boundary_width_trench * forearc_width_scale,
                                );
                                let forearc_near_trench = band_weight(
                                    d,
                                    params.boundary_width_trench * 1.05,
                                    params.boundary_anisotropy * 0.6,
                                );
                                delta[v] -= 0.06 * conv_base * forearc_near_trench;
                                delta[v] -= 0.07 * conv_base * forearc_w;

                                let arc_center =
                                    params.boundary_width_arc
                                        * arc_offset_scale
                                        * (0.9 + 0.4 * edge.obliquity);
                                let arc_w = ring_weight(
                                    d,
                                    arc_center,
                                    params.boundary_width_arc * 0.7,
                                );
                                let arc_gain = params.arc_gain;
                                let (arc_multi_w, arc_multi_dist_scale) =
                                    accumulate_multi_edge_arc_signal(
                                        v,
                                        pid,
                                        positions,
                                        nbr_offsets,
                                        nbrs,
                                        &nearest_edge,
                                        boundary_edges,
                                        attributes,
                                        vertex_lithosphere,
                                        params,
                                    );
                                let arc_apply_w = arc_multi_w.max(arc_w);
                                let arc_apply_dist_scale = arc_multi_dist_scale.max(dist_scale);
                                delta[v] += conv_base * arc_gain * arc_apply_w * arc_apply_dist_scale;
                                let arc_debug_strength = (edge.strength * arc_apply_w).min(1.0);
                                debug_arc_strength[v] = debug_arc_strength[v].max(arc_debug_strength);
                                if matches!(mode, ConvergentMode::OceanOcean) {
                                    debug_ocean_ocean_arc_strength[v] =
                                        debug_ocean_ocean_arc_strength[v].max(arc_debug_strength);
                                }

                                let backarc_center = arc_center
                                    + params.boundary_width_arc
                                        * backarc_offset_scale
                                        * (0.9 + 0.2 * edge.obliquity);
                                let backarc_w = ring_weight(
                                    d,
                                    backarc_center,
                                    params.boundary_width_arc * backarc_width_scale,
                                );
                                let backarc_basin_gain = if matches!(mode, ConvergentMode::OceanOcean) {
                                    0.10
                                } else {
                                    0.15
                                };
                                let backarc_shoulder_w = ring_weight(
                                    d,
                                    backarc_center + params.boundary_width_arc * 0.75,
                                    params.boundary_width_arc * 0.75,
                                );
                                delta[v] -= conv_base * params.arc_gain * backarc_basin_gain * backarc_w * dist_scale;
                                delta[v] +=
                                    0.08 * conv_base * params.arc_gain * backarc_shoulder_w * dist_scale;
                                debug_backarc_strength[v] =
                                    debug_backarc_strength[v].max((edge.strength * backarc_w).min(1.0));

                                let plateau_center = backarc_center
                                    + params.boundary_width_arc
                                        * lerp(1.35, 0.90, subduction_angle)
                                        * (0.95 + 0.15 * edge.obliquity);
                                let plateau_w = ring_weight(
                                    d,
                                    plateau_center,
                                    params.boundary_width_arc * lerp(2.10, 1.45, subduction_angle),
                                );
                                let plateau_inner_shadow = band_weight(
                                    d,
                                    params.boundary_width_trench
                                        * trench_width_scale
                                        * (1.55 + 0.15 * edge.obliquity),
                                    params.boundary_anisotropy * 0.45,
                                );
                                let plateau_gain = if matches!(mode, ConvergentMode::OceanOcean) {
                                    0.12
                                } else {
                                    0.18
                                };
                                let plateau_uplift = conv_base
                                    * params.arc_gain
                                    * plateau_gain
                                    * plateau_w
                                    * dist_scale
                                    * (1.0 - 0.55 * plateau_inner_shadow);
                                delta[v] += plateau_uplift;
                                preserve_strength[v] =
                                    preserve_strength[v]
                                        .max(0.85 * forearc_w.max(arc_apply_w).max(backarc_w))
                                        .max(0.55 * plateau_w);
                            }
                        }
                    }
                }
            }
            BoundaryType::Divergent => {
                let mut rift_width = params.boundary_width_rift;
                if !attributes[edge.plate_a].is_ocean && !attributes[edge.plate_b].is_ocean {
                    rift_width *= 1.35;
                }
                let oblique_relief = 1.0 - 0.6 * params.boundary_obliquity_mix * edge.obliquity;
                let rift_w = band_weight(d, rift_width, params.boundary_anisotropy * 0.8);
                let rift = params.boundary_divergent_base_gain
                    * params.rift_gain
                    * edge.strength
                    * oblique_relief
                    * rift_w;
                delta[v] -= rift;
                if d < rift_width * 0.65 {
                    delta[v] -=
                        0.05 * params.boundary_divergent_base_gain * edge.strength * rift_w;
                }
                preserve_strength[v] = preserve_strength[v].max(0.55 * rift_w);
            }
            BoundaryType::Transform => {
                let width = params.boundary_width_trench * 0.9;
                let w = band_weight(d, width, params.boundary_anisotropy * 0.5);
                let sign = if ((v as u32).wrapping_mul(1103515245) ^ (edge_idx as u32)) & 1 == 0 {
                    1.0
                } else {
                    -1.0
                };
                let relief = params.boundary_transform_relief_gain
                    * edge.strength
                    * (1.0 + 0.4 * edge.obliquity)
                    * w;
                delta[v] += sign * 0.5 * relief;
                preserve_strength[v] = preserve_strength[v].max(0.35 * w);
            }
        }
    }

    for v in 0..height.len() {
        let boosted = if boundary_vertices.mask[v] { delta[v] * 1.20 } else { delta[v] };
        height[v] = clamp(height[v] + boosted, -1.0, 1.0);
    }

    BoundaryFields {
        preserve_strength,
        debug_trench_strength,
        debug_arc_strength,
        debug_backarc_strength,
        debug_ocean_ocean_arc_strength,
    }
}

fn apply_intraplate_fold_belts(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    attributes: &[PlateAttr],
    vertex_lithosphere: &[VertexLithosphere],
    boundary_edges: &[BoundaryEdge],
    height: &mut [f32],
    boundary_fields: &mut BoundaryFields,
    params: &TerrainParams,
) {
    if boundary_edges.is_empty() || height.is_empty() {
        return;
    }

    let fold_sources = collect_intraplate_fold_sources(boundary_edges, attributes);
    if fold_sources.is_empty() {
        return;
    }

    let (nearest_source, source_dist) = compute_continental_stress_assignment(
        positions,
        nbr_offsets,
        nbrs,
        plate_id,
        attributes,
        &fold_sources,
    );

    let near_quiet = params.boundary_width_collision * 0.55;
    let inland_ramp = (params.boundary_width_collision * 1.8).max(near_quiet + 1e-3);
    let stress_falloff = 2.4 / (params.boundary_width_collision + 0.20);
    let fold_gain = 0.055 * (0.7 + 0.6 * params.collision_gain);
    let preserve_gain = 0.55;

    for v in 0..height.len() {
        let pid = plate_id[v] as usize;
        if pid >= attributes.len() || attributes[pid].is_ocean {
            continue;
        }

        let source_idx = nearest_source[v];
        if source_idx == usize::MAX {
            continue;
        }

        let d = source_dist[v];
        let inland_gate = smoothstep01((d - near_quiet) / (inland_ramp - near_quiet));
        if inland_gate <= 0.0 {
            continue;
        }

        let source = fold_sources[source_idx];
        let stress_mag = source.seed_strength * inland_gate * (-(d * stress_falloff)).exp();
        if stress_mag < 0.015 {
            continue;
        }

        let plate_velocity = project_to_tangent(attributes[pid].velocity, positions[v]);
        let comp_dir = normalize3(plate_velocity);
        if length3(comp_dir) <= 1e-5 {
            continue;
        }

        let edge = source.edge;
        let edge_tangent = normalize3(project_to_tangent(sub3(positions[edge.b], positions[edge.a]), positions[v]));
        if length3(edge_tangent) <= 1e-5 {
            continue;
        }

        let edge_mid = normalize3(add3(positions[edge.a], positions[edge.b]));
        let rel = project_to_tangent(sub3(positions[v], edge_mid), positions[v]);
        let across = dot3(rel, comp_dir);
        let along = dot3(rel, edge_tangent);

        let source_hash = trig_hash01(positions[v], source_idx as u32);
        let wavelength = lerp(0.10, 0.22, source_hash);
        let base_phase = (across / wavelength) * std::f32::consts::TAU;
        let phase_offset = source.phase * std::f32::consts::TAU;
        let harmonic_phase = phase_offset + 0.35 * along / (wavelength * 1.3 + 1e-4);
        let fold_pattern = 0.72 * (base_phase + phase_offset).sin()
            + 0.28 * (2.1 * base_phase + harmonic_phase).sin();

        let weakness = 0.70 + 0.30 * trig_hash01(positions[v], (source_idx as u32) ^ 0x9e37_79b9);
        let uplift_bias = 0.22;
        let mode_gain = source.mode_gain;
        let competence = vertex_lithosphere
            .get(v)
            .map(|lith| lith.competence)
            .unwrap_or(0.5);
        let foldability = foldability_from_competence(competence, params);
        let delta =
            fold_gain * mode_gain * stress_mag * weakness * foldability * (fold_pattern + uplift_bias);
        height[v] = clamp(height[v] + delta, -1.0, 1.0);

        let preserve = clamp(
            preserve_gain * stress_mag * (0.35 + 0.65 * fold_pattern.abs()),
            0.0,
            0.70,
        );
        boundary_fields.preserve_strength[v] = boundary_fields.preserve_strength[v].max(preserve);
    }
}

fn foldability_from_competence(competence: f32, params: &TerrainParams) -> f32 {
    let influence = clamp(params.continent_foldability_from_competence, 0.0, 1.0);
    let inverse_comp = 1.0 - clamp(competence, 0.0, 1.0);
    lerp(1.0, inverse_comp, influence)
}

fn extract_boundary_edges(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    attributes: &[PlateAttr],
) -> Vec<BoundaryEdge> {
    let mut edges = Vec::new();
    let classify_eps = 0.05;

    for i in 0..positions.len() {
        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        for &j_u32 in &nbrs[start..end] {
            let j = j_u32 as usize;
            if j <= i {
                continue;
            }

            let plate_a = plate_id[i] as usize;
            let plate_b = plate_id[j] as usize;
            if plate_a == plate_b {
                continue;
            }

            let edge_vec = sub3(positions[j], positions[i]);
            let edge_dir = normalize3(edge_vec);
            let rel_v = sub3(attributes[plate_b].velocity, attributes[plate_a].velocity);
            let v_rel_n = dot3(rel_v, edge_dir);
            let v_rel_t_vec = sub3(rel_v, mul3(edge_dir, v_rel_n));
            let v_rel_t = length3(v_rel_t_vec);
            let obliquity = v_rel_t / (v_rel_t + v_rel_n.abs() + 1e-5);
            let (boundary_type, strength) = if v_rel_n > classify_eps {
                (
                    BoundaryType::Convergent,
                    clamp((v_rel_n - classify_eps) / 0.25, 0.0, 1.0),
                )
            } else if v_rel_n < -classify_eps {
                (
                    BoundaryType::Divergent,
                    clamp((-v_rel_n - classify_eps) / 0.25, 0.0, 1.0),
                )
            } else {
                (
                    BoundaryType::Transform,
                    clamp((v_rel_t - 0.02) / 0.18, 0.0, 1.0),
                )
            };

            edges.push(BoundaryEdge {
                a: i,
                b: j,
                plate_a,
                plate_b,
                boundary_type,
                strength: strength.max(0.05),
                obliquity,
            });
        }
    }

    edges
}

#[derive(Clone, Copy)]
struct IntraplateFoldSource {
    edge: BoundaryEdge,
    seed_strength: f32,
    mode_gain: f32,
    phase: f32,
}

fn collect_intraplate_fold_sources(
    boundary_edges: &[BoundaryEdge],
    attributes: &[PlateAttr],
) -> Vec<IntraplateFoldSource> {
    let mut sources = Vec::new();
    for (idx, edge) in boundary_edges.iter().enumerate() {
        if !matches!(edge.boundary_type, BoundaryType::Convergent) {
            continue;
        }

        let a_ocean = attributes[edge.plate_a].is_ocean;
        let b_ocean = attributes[edge.plate_b].is_ocean;
        if a_ocean && b_ocean {
            continue;
        }

        let both_continent = !a_ocean && !b_ocean;
        let mode_gain = if both_continent { 1.0 } else { 0.70 };
        let seed_strength = edge.strength * (1.0 - 0.35 * edge.obliquity);
        if seed_strength <= 0.04 {
            continue;
        }

        let phase = fract01((idx as f32) * 0.618_034 + edge.obliquity * 0.37);
        sources.push(IntraplateFoldSource {
            edge: *edge,
            seed_strength,
            mode_gain,
            phase,
        });
    }
    sources
}

fn classify_convergent_edge(
    vertex_a: usize,
    vertex_b: usize,
    plate_a: usize,
    plate_b: usize,
    attributes: &[PlateAttr],
    vertex_lithosphere: &[VertexLithosphere],
) -> (Option<ConvergentMode>, SubductionPolarity) {
    let a_ocean = attributes[plate_a].is_ocean;
    let b_ocean = attributes[plate_b].is_ocean;

    if a_ocean && !b_ocean {
        return (Some(ConvergentMode::OceanContinent), SubductionPolarity::AUnderB);
    }
    if !a_ocean && b_ocean {
        return (Some(ConvergentMode::OceanContinent), SubductionPolarity::BUnderA);
    }
    if a_ocean && b_ocean {
        let a_weight = vertex_lithosphere[vertex_a].weight;
        let b_weight = vertex_lithosphere[vertex_b].weight;
        let polarity = if a_weight >= b_weight {
            SubductionPolarity::AUnderB
        } else {
            SubductionPolarity::BUnderA
        };
        return (Some(ConvergentMode::OceanOcean), polarity);
    }

    (Some(ConvergentMode::ContinentContinent), SubductionPolarity::None)
}

fn estimate_subduction_angle_proxy(
    edge: BoundaryEdge,
    polarity: SubductionPolarity,
    attributes: &[PlateAttr],
    vertex_lithosphere: &[VertexLithosphere],
) -> f32 {
    let (subducting_vertex, overriding_vertex, subducting_plate, overriding_plate) = match polarity {
        SubductionPolarity::AUnderB => (edge.a, edge.b, edge.plate_a, edge.plate_b),
        SubductionPolarity::BUnderA => (edge.b, edge.a, edge.plate_b, edge.plate_a),
        SubductionPolarity::None => return 0.5,
    };

    if subducting_vertex >= vertex_lithosphere.len() || overriding_vertex >= vertex_lithosphere.len() {
        return 0.5;
    }
    if subducting_plate >= attributes.len() || overriding_plate >= attributes.len() {
        return 0.5;
    }

    let sub = vertex_lithosphere[subducting_vertex];
    let over = vertex_lithosphere[overriding_vertex];

    let sub_age = clamp(sub.age_norm, 0.0, 1.0);
    let sub_weight = clamp(sub.weight, 0.0, 1.0);
    let over_buoyancy = clamp(over.buoyancy, -1.0, 1.0);
    let over_resistance = 0.5 + 0.5 * over_buoyancy;
    let convergence_component = edge.strength * (1.0 - 0.35 * edge.obliquity);
    let ocean_ocean_bonus =
        if attributes[subducting_plate].is_ocean && attributes[overriding_plate].is_ocean {
            0.06
        } else {
            0.0
        };

    clamp(
        0.34
            + 0.28 * sub_age
            + 0.20 * sub_weight
            + 0.14 * convergence_component
            - 0.08 * over_resistance
            + ocean_ocean_bonus,
        0.0,
        1.0,
    )
}

fn accumulate_multi_edge_arc_signal(
    vertex: usize,
    pid: usize,
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    nearest_edge: &[usize],
    boundary_edges: &[BoundaryEdge],
    attributes: &[PlateAttr],
    vertex_lithosphere: &[VertexLithosphere],
    params: &TerrainParams,
) -> (f32, f32) {
    let mut candidates = Vec::with_capacity(24);
    push_unique_edge_candidate(&mut candidates, nearest_edge[vertex]);

    let start = nbr_offsets[vertex] as usize;
    let end = nbr_offsets[vertex + 1] as usize;
    for &n_u32 in &nbrs[start..end] {
        let n = n_u32 as usize;
        push_unique_edge_candidate(&mut candidates, nearest_edge[n]);

        let n_start = nbr_offsets[n] as usize;
        let n_end = nbr_offsets[n + 1] as usize;
        for &n2_u32 in &nbrs[n_start..n_end] {
            push_unique_edge_candidate(&mut candidates, nearest_edge[n2_u32 as usize]);
            if candidates.len() >= 24 {
                break;
            }
        }
        if candidates.len() >= 24 {
            break;
        }
    }

    let mut weight_sum = 0.0;
    let mut dist_scale_sum = 0.0;
    let mut contrib_sum = 0.0;
    let pos_v = positions[vertex];

    for &edge_idx in &candidates {
        if edge_idx == usize::MAX || edge_idx >= boundary_edges.len() {
            continue;
        }
        let edge = boundary_edges[edge_idx];
        if !matches!(edge.boundary_type, BoundaryType::Convergent) {
            continue;
        }

        let (convergent_mode, polarity) = classify_convergent_edge(
            edge.a,
            edge.b,
            edge.plate_a,
            edge.plate_b,
            attributes,
            vertex_lithosphere,
        );
        let Some(mode) = convergent_mode else {
            continue;
        };
        if !matches!(mode, ConvergentMode::OceanContinent | ConvergentMode::OceanOcean) {
            continue;
        }

        let overriding = match polarity {
            SubductionPolarity::AUnderB => edge.plate_b,
            SubductionPolarity::BUnderA => edge.plate_a,
            SubductionPolarity::None => usize::MAX,
        };
        if pid != overriding {
            continue;
        }

        let subduction_angle =
            estimate_subduction_angle_proxy(edge, polarity, attributes, vertex_lithosphere);
        let arc_offset_scale = lerp(1.22, 0.82, subduction_angle);
        let arc_center = params.boundary_width_arc * arc_offset_scale * (0.9 + 0.4 * edge.obliquity);

        let edge_mid = normalize3(add3(positions[edge.a], positions[edge.b]));
        let d_mid = chord_distance(pos_v, edge_mid);
        let d_end =
            chord_distance(pos_v, positions[edge.a]).min(chord_distance(pos_v, positions[edge.b]));
        let d = d_mid.min(d_end * 0.9);
        let arc_w = ring_weight(d, arc_center, params.boundary_width_arc * 0.70);
        if arc_w <= 1e-4 {
            continue;
        }

        let dist_scale = (-(d * params.boundary_distance_falloff)).exp();
        let source_weight = edge.strength * (1.0 - 0.45 * edge.obliquity);
        let w = arc_w * source_weight.max(0.05);
        contrib_sum += arc_w * w;
        dist_scale_sum += dist_scale * w;
        weight_sum += w;
    }

    if weight_sum <= 1e-6 {
        (0.0, 0.0)
    } else {
        (contrib_sum / weight_sum, dist_scale_sum / weight_sum)
    }
}

fn push_unique_edge_candidate(candidates: &mut Vec<usize>, edge_idx: usize) {
    if edge_idx == usize::MAX {
        return;
    }
    if candidates.iter().any(|&x| x == edge_idx) {
        return;
    }
    candidates.push(edge_idx);
}

fn compute_continental_stress_assignment(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    attributes: &[PlateAttr],
    fold_sources: &[IntraplateFoldSource],
) -> (Vec<usize>, Vec<f32>) {
    let vertex_count = positions.len();
    let mut nearest_source = vec![usize::MAX; vertex_count];
    let mut dist = vec![f32::INFINITY; vertex_count];
    let mut heap = BinaryHeap::new();

    for (source_idx, source) in fold_sources.iter().enumerate() {
        for &v in &[source.edge.a, source.edge.b] {
            let pid = plate_id[v] as usize;
            if pid >= attributes.len() || attributes[pid].is_ocean {
                continue;
            }
            if dist[v] > 0.0 {
                dist[v] = 0.0;
                nearest_source[v] = source_idx;
                heap.push(BoundaryDistState {
                    cost: 0.0,
                    vertex: v,
                    source_edge: source_idx,
                });
            }
        }
    }

    while let Some(state) = heap.pop() {
        if state.cost > dist[state.vertex] + 1e-6 {
            continue;
        }

        let start = nbr_offsets[state.vertex] as usize;
        let end = nbr_offsets[state.vertex + 1] as usize;
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            let npid = plate_id[n] as usize;
            if npid >= attributes.len() || attributes[npid].is_ocean {
                continue;
            }

            let mut step = chord_distance(positions[state.vertex], positions[n]).max(1e-4);
            if plate_id[n] != plate_id[state.vertex] {
                step *= 1.35;
            }
            let next_cost = state.cost + step;
            if next_cost + 1e-6 < dist[n] {
                dist[n] = next_cost;
                nearest_source[n] = state.source_edge;
                heap.push(BoundaryDistState {
                    cost: next_cost,
                    vertex: n,
                    source_edge: state.source_edge,
                });
            }
        }
    }

    (nearest_source, dist)
}

fn compute_boundary_distance_assignment(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    boundary_edges: &[BoundaryEdge],
    vertex_count: usize,
) -> (Vec<usize>, Vec<f32>, BoundaryVertices) {
    let mut nearest_edge = vec![usize::MAX; vertex_count];
    let mut dist = vec![f32::INFINITY; vertex_count];
    let mut boundary_vertices = BoundaryVertices::new(vertex_count);
    let mut heap = BinaryHeap::new();

    for (edge_idx, edge) in boundary_edges.iter().enumerate() {
        for &v in &[edge.a, edge.b] {
            boundary_vertices.insert(v);
            if 0.0 < dist[v] {
                dist[v] = 0.0;
                nearest_edge[v] = edge_idx;
                heap.push(BoundaryDistState {
                    cost: 0.0,
                    vertex: v,
                    source_edge: edge_idx,
                });
            }
        }
    }

    while let Some(state) = heap.pop() {
        if state.cost > dist[state.vertex] + 1e-6 {
            continue;
        }

        let start = nbr_offsets[state.vertex] as usize;
        let end = nbr_offsets[state.vertex + 1] as usize;
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            let step = chord_distance(positions[state.vertex], positions[n]).max(1e-4);
            let next_cost = state.cost + step;
            if next_cost + 1e-6 < dist[n] {
                dist[n] = next_cost;
                nearest_edge[n] = state.source_edge;
                heap.push(BoundaryDistState {
                    cost: next_cost,
                    vertex: n,
                    source_edge: state.source_edge,
                });
            }
        }
    }

    (nearest_edge, dist, boundary_vertices)
}

fn band_weight(distance: f32, width: f32, anisotropy: f32) -> f32 {
    let sigma = (width * (1.0 - 0.35 * anisotropy)).max(1e-4);
    (-(distance * distance) / (2.0 * sigma * sigma)).exp()
}

fn ring_weight(distance: f32, center: f32, width: f32) -> f32 {
    let sigma = width.max(1e-4);
    let dx = distance - center;
    (-(dx * dx) / (2.0 * sigma * sigma)).exp()
}

fn smoothstep01(t: f32) -> f32 {
    let x = clamp(t, 0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn fract01(v: f32) -> f32 {
    v - v.floor()
}

fn trig_hash01(pos: [f32; 3], seed: u32) -> f32 {
    let seedf = seed as f32;
    let s = (pos[0] * 12.9898 + pos[1] * 78.233 + pos[2] * 37.719 + seedf * 0.12345).sin();
    fract01(s * 43_758.547)
}
