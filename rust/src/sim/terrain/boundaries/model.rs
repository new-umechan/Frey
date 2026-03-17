use super::*;

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
                            let w = band_weight(d, params.boundary_collision_width, params.boundary_anisotropy);
                            let uplift = conv_base * params.collision_gain * w;
                            delta[v] += uplift;
                            if d < params.boundary_collision_width * 0.55 {
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
                                    params.boundary_trench_width
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
                                    params.boundary_trench_width * trench_width_scale * 1.6,
                                    params.boundary_trench_width * 0.65,
                                );
                                delta[v] += 0.12 * conv_base * outer_rise * dist_scale;
                                preserve_strength[v] = preserve_strength[v].max(0.95 * trench_w);
                            } else if pid == overriding {
                                let forearc_center = params.boundary_trench_width
                                    * trench_width_scale
                                    * forearc_offset_scale
                                    * (1.0 + 0.15 * edge.obliquity);
                                let forearc_w = ring_weight(
                                    d,
                                    forearc_center,
                                    params.boundary_trench_width * forearc_width_scale,
                                );
                                let forearc_near_trench = band_weight(
                                    d,
                                    params.boundary_trench_width * 1.05,
                                    params.boundary_anisotropy * 0.6,
                                );
                                delta[v] -= 0.06 * conv_base * forearc_near_trench;
                                delta[v] -= 0.07 * conv_base * forearc_w;

                                let arc_center =
                                    params.boundary_arc_width
                                        * arc_offset_scale
                                        * (0.9 + 0.4 * edge.obliquity);
                                let arc_w = ring_weight(
                                    d,
                                    arc_center,
                                    params.boundary_arc_width * 0.7,
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
                                    + params.boundary_arc_width
                                        * backarc_offset_scale
                                        * (0.9 + 0.2 * edge.obliquity);
                                let backarc_w = ring_weight(
                                    d,
                                    backarc_center,
                                    params.boundary_arc_width * backarc_width_scale,
                                );
                                let backarc_basin_gain = if matches!(mode, ConvergentMode::OceanOcean) {
                                    0.10
                                } else {
                                    0.15
                                };
                                let backarc_shoulder_w = ring_weight(
                                    d,
                                    backarc_center + params.boundary_arc_width * 0.75,
                                    params.boundary_arc_width * 0.75,
                                );
                                delta[v] -= conv_base * params.arc_gain * backarc_basin_gain * backarc_w * dist_scale;
                                delta[v] +=
                                    0.08 * conv_base * params.arc_gain * backarc_shoulder_w * dist_scale;
                                debug_backarc_strength[v] =
                                    debug_backarc_strength[v].max((edge.strength * backarc_w).min(1.0));

                                let plateau_center = backarc_center
                                    + params.boundary_arc_width
                                        * lerp(1.35, 0.90, subduction_angle)
                                        * (0.95 + 0.15 * edge.obliquity);
                                let plateau_w = ring_weight(
                                    d,
                                    plateau_center,
                                    params.boundary_arc_width * lerp(2.10, 1.45, subduction_angle),
                                );
                                let plateau_inner_shadow = band_weight(
                                    d,
                                    params.boundary_trench_width
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
                let mut rift_width = params.boundary_rift_width;
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
                let width = params.boundary_trench_width * 0.9;
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

    let near_quiet = params.boundary_collision_width * 0.55;
    let inland_ramp = (params.boundary_collision_width * 1.8).max(near_quiet + 1e-3);
    let stress_falloff = 2.4 / (params.boundary_collision_width + 0.20);
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

        let plate_velocity = local_plate_velocity(&attributes[pid], pid, positions[v]);
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

