use super::*;
use crate::sim::geology_types::PlateId;

const EARTH_RADIUS_KM: f32 = 6_371.0;
const SUBDUCTION_ARC_DEPTH_MIN_KM: f32 = 90.0;
const SUBDUCTION_ARC_DEPTH_MAX_KM: f32 = 130.0;
const SUBDUCTION_DIP_MIN_DEG: f32 = 25.0;
const SUBDUCTION_DIP_MAX_DEG: f32 = 65.0;

pub(super) struct BoundaryModelInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub nbr_offsets: &'a [u32],
    pub nbrs: &'a [u32],
    pub plate_id: &'a [PlateId],
    pub attributes: &'a [PlateAttr],
    pub vertex_lithosphere: &'a [VertexLithosphere],
    pub boundary_edges: &'a [BoundaryEdge],
    pub params: &'a GeologyParams,
}

pub(super) fn apply_boundary_model(
    input: BoundaryModelInput<'_>,
    height: &mut [f32],
) -> BoundaryFields {
    let positions = input.positions;
    let nbr_offsets = input.nbr_offsets;
    let nbrs = input.nbrs;
    let plate_id = input.plate_id;
    let attributes = input.attributes;
    let vertex_lithosphere = input.vertex_lithosphere;
    let boundary_edges = input.boundary_edges;
    let params = input.params;

    if boundary_edges.is_empty() {
        return BoundaryFields {
            preserve_strength: vec![0.0; height.len()],
            debug_trench_strength: vec![0.0; height.len()],
            debug_arc_strength: vec![0.0; height.len()],
            debug_backarc_strength: vec![0.0; height.len()],
            debug_ocean_ocean_arc_strength: vec![0.0; height.len()],
        };
    }

    let (nearest_edge, boundary_dist, boundary_vertices) = compute_boundary_distance_assignment(
        positions,
        nbr_offsets,
        nbrs,
        boundary_edges,
        height.len(),
    );

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
        let pid = plate_id[v].as_usize();
        let d = boundary_dist[v];
        let dist_scale = (-(d * params.boundary_distance_falloff)).exp();

        match edge.boundary_type {
            EdgeReliefType::Convergent => {
                let oblique_relief = 1.0 - params.boundary_obliquity_mix * edge.obliquity;
                let convergence = edge.convergence.max(edge.strength * 0.35).clamp(0.0, 1.0);
                let conv_base = params.boundary_convergent_base_gain
                    * convergence
                    * oblique_relief.clamp(0.0, 1.0);

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
                            let suture_w = band_weight(
                                d,
                                params.boundary_collision_width * 0.35,
                                params.boundary_anisotropy * 0.65,
                            );
                            let core_w = ring_weight(
                                d,
                                params.boundary_collision_width * 0.45,
                                params.boundary_collision_width * 0.75,
                            );
                            let plateau_w = band_weight(
                                d,
                                params.boundary_collision_width * 1.8,
                                params.boundary_anisotropy * 1.35,
                            );
                            let orogen_w = 0.35 * core_w + 0.65 * plateau_w;
                            let uplift = conv_base * params.collision_gain * orogen_w;
                            let suture_notch = 0.08 * conv_base * params.collision_gain * suture_w;
                            delta[v] += uplift - suture_notch;
                            preserve_strength[v] =
                                preserve_strength[v].max(0.55 * orogen_w.max(suture_w * 0.5));
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
                            let subduction_gate = subduction_gate_for_edge(
                                edge,
                                subduction_polarity,
                                vertex_lithosphere,
                            );
                            let trench_depth_scale = lerp(0.90, 1.28, subduction_angle);
                            let trench_width_scale = lerp(1.16, 0.88, subduction_angle);
                            let arc_center =
                                subduction_arc_center(subduction_angle, subduction_gate);
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
                                let trench = conv_base
                                    * params.trench_gain
                                    * subduction_gate
                                    * trench_depth_scale
                                    * trench_w;
                                delta[v] -= trench;
                                debug_trench_strength[v] = debug_trench_strength[v]
                                    .max((subduction_gate * convergence * trench_w).min(1.0));
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

                                let arc_center = arc_center * (0.95 + 0.15 * edge.obliquity);
                                let arc_width = (arc_center * 0.35)
                                    .max(params.boundary_arc_width * 0.45)
                                    .min(params.boundary_arc_width * 1.15);
                                let arc_w = ring_weight(d, arc_center, arc_width);
                                let arc_gain = params.arc_gain;
                                let (arc_multi_w, arc_multi_dist_scale) =
                                    accumulate_multi_edge_arc_signal(ArcSignalInput {
                                        vertex: v,
                                        pid,
                                        positions,
                                        nbr_offsets,
                                        nbrs,
                                        nearest_edge: &nearest_edge,
                                        boundary_edges,
                                        attributes,
                                        vertex_lithosphere,
                                        params,
                                    });
                                let arc_apply_w = arc_multi_w.max(arc_w);
                                let arc_apply_dist_scale = arc_multi_dist_scale.max(dist_scale);
                                delta[v] += conv_base
                                    * arc_gain
                                    * subduction_gate
                                    * arc_apply_w
                                    * arc_apply_dist_scale;
                                let arc_debug_strength =
                                    (subduction_gate * convergence * arc_apply_w).min(1.0);
                                debug_arc_strength[v] =
                                    debug_arc_strength[v].max(arc_debug_strength);
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
                                let backarc_basin_gain =
                                    if matches!(mode, ConvergentMode::OceanOcean) {
                                        0.10
                                    } else {
                                        0.15
                                    };
                                let rollback_proxy = (subduction_gate
                                    * (0.55 + 0.45 * edge.obliquity))
                                    .clamp(0.0, 1.0);
                                let backarc_shoulder_w = ring_weight(
                                    d,
                                    backarc_center + params.boundary_arc_width * 0.75,
                                    params.boundary_arc_width * 0.75,
                                );
                                delta[v] -= conv_base
                                    * params.arc_gain
                                    * rollback_proxy
                                    * backarc_basin_gain
                                    * backarc_w
                                    * dist_scale;
                                delta[v] += 0.08
                                    * conv_base
                                    * params.arc_gain
                                    * backarc_shoulder_w
                                    * dist_scale;
                                debug_backarc_strength[v] = debug_backarc_strength[v]
                                    .max((edge.strength * backarc_w).min(1.0));

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
                                preserve_strength[v] = preserve_strength[v]
                                    .max(0.85 * forearc_w.max(arc_apply_w).max(backarc_w))
                                    .max(0.55 * plateau_w);
                            }
                        }
                    }
                }
            }
            EdgeReliefType::Divergent => {
                let mut rift_width = params.boundary_rift_width;
                if !attributes[edge.plate_a].is_ocean && !attributes[edge.plate_b].is_ocean {
                    rift_width *= 1.35;
                }
                let oblique_relief = 1.0 - 0.6 * params.boundary_obliquity_mix * edge.obliquity;
                let rift_w = band_weight(d, rift_width, params.boundary_anisotropy * 0.8);
                let divergence = edge.divergence.max(edge.strength * 0.35).clamp(0.0, 1.0);
                let rift = params.boundary_divergent_base_gain
                    * params.rift_gain
                    * divergence
                    * oblique_relief
                    * rift_w;
                delta[v] -= rift;
                if d < rift_width * 0.65 {
                    delta[v] -= 0.05 * params.boundary_divergent_base_gain * divergence * rift_w;
                }
                if attributes[edge.plate_a].is_ocean || attributes[edge.plate_b].is_ocean {
                    let ridge_w = band_weight(
                        d,
                        params.boundary_trench_width.max(params.boundary_band),
                        params.boundary_anisotropy * 0.7,
                    );
                    delta[v] += 0.30 * params.boundary_divergent_base_gain * divergence * ridge_w;
                }
                preserve_strength[v] = preserve_strength[v].max(0.55 * rift_w);
            }
            EdgeReliefType::Transform => {
                let width = params.boundary_trench_width * 0.9;
                let w = band_weight(d, width, params.boundary_anisotropy * 0.5);
                let sign = if ((v as u32).wrapping_mul(1103515245) ^ (edge_idx as u32)) & 1 == 0 {
                    1.0
                } else {
                    -1.0
                };
                let transform = edge.transform.max(edge.strength * 0.35).clamp(0.0, 1.0);
                let relief = params.boundary_transform_relief_gain
                    * transform
                    * (1.0 + 0.4 * edge.obliquity)
                    * w;
                delta[v] += sign * 0.5 * relief;
                preserve_strength[v] = preserve_strength[v].max(0.35 * w);
            }
        }
    }

    for v in 0..height.len() {
        let boosted = if boundary_vertices.mask[v] {
            delta[v] * 1.20
        } else {
            delta[v]
        };
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

pub(super) struct IntraplateFoldInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub nbr_offsets: &'a [u32],
    pub nbrs: &'a [u32],
    pub plate_id: &'a [PlateId],
    pub attributes: &'a [PlateAttr],
    pub vertex_lithosphere: &'a [VertexLithosphere],
    pub boundary_edges: &'a [BoundaryEdge],
    pub params: &'a GeologyParams,
}

pub(super) fn apply_intraplate_fold_belts(
    input: IntraplateFoldInput<'_>,
    height: &mut [f32],
    boundary_fields: &mut BoundaryFields,
) {
    let positions = input.positions;
    let nbr_offsets = input.nbr_offsets;
    let nbrs = input.nbrs;
    let plate_id = input.plate_id;
    let attributes = input.attributes;
    let vertex_lithosphere = input.vertex_lithosphere;
    let boundary_edges = input.boundary_edges;
    let params = input.params;

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
        let pid = plate_id[v].as_usize();
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
        let edge_tangent = normalize3(project_to_tangent(
            sub3(positions[edge.b], positions[edge.a]),
            positions[v],
        ));
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
        let delta = fold_gain
            * mode_gain
            * stress_mag
            * weakness
            * foldability
            * (fold_pattern + uplift_bias);
        height[v] = clamp(height[v] + delta, -1.0, 1.0);

        let preserve = clamp(
            preserve_gain * stress_mag * (0.35 + 0.65 * fold_pattern.abs()),
            0.0,
            0.70,
        );
        boundary_fields.preserve_strength[v] = boundary_fields.preserve_strength[v].max(preserve);
    }
}

pub(super) fn foldability_from_competence(competence: f32, params: &GeologyParams) -> f32 {
    let influence = clamp(params.continent_foldability_from_competence, 0.0, 1.0);
    let inverse_comp = 1.0 - clamp(competence, 0.0, 1.0);
    lerp(1.0, inverse_comp, influence)
}

pub(super) fn extract_boundary_edges(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
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

            let plate_a = plate_id[i].as_usize();
            let plate_b = plate_id[j].as_usize();
            if plate_a == plate_b {
                continue;
            }

            let edge_vec = sub3(positions[j], positions[i]);
            let edge_dir = normalize3(edge_vec);
            let vel_a = local_plate_velocity(&attributes[plate_a], plate_a, positions[i]);
            let vel_b = local_plate_velocity(&attributes[plate_b], plate_b, positions[j]);
            let rel_v = sub3(vel_b, vel_a);
            let rel_n = dot3(rel_v, edge_dir);
            let rel_t_vec = sub3(rel_v, mul3(edge_dir, rel_n));
            let rel_t = length3(rel_t_vec);
            let convergence = rel_n.max(0.0);
            let divergence = (-rel_n).max(0.0);
            let obliquity = rel_t / (convergence + divergence + rel_t + 1e-5);
            let convergence_norm = clamp((convergence - classify_eps) / 0.25, 0.0, 1.0);
            let divergence_norm = clamp((divergence - classify_eps) / 0.25, 0.0, 1.0);
            let transform_norm = clamp((rel_t - 0.02) / 0.18, 0.0, 1.0);
            let motion_strength = clamp((convergence + divergence + 0.5 * rel_t) / 0.25, 0.0, 1.0);
            let (boundary_type, directional_strength) = if rel_n > classify_eps {
                (EdgeReliefType::Convergent, convergence_norm)
            } else if rel_n < -classify_eps {
                (EdgeReliefType::Divergent, divergence_norm)
            } else {
                (EdgeReliefType::Transform, transform_norm)
            };

            edges.push(BoundaryEdge {
                a: i,
                b: j,
                plate_a,
                plate_b,
                boundary_type,
                strength: directional_strength.max(0.30 * motion_strength).max(0.05),
                obliquity,
                convergence: convergence_norm,
                divergence: divergence_norm,
                transform: transform_norm,
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
        if !matches!(edge.boundary_type, EdgeReliefType::Convergent) {
            continue;
        }

        let a_ocean = attributes[edge.plate_a].is_ocean;
        let b_ocean = attributes[edge.plate_b].is_ocean;
        if a_ocean && b_ocean {
            continue;
        }

        let both_continent = !a_ocean && !b_ocean;
        let mode_gain = if both_continent { 1.0 } else { 0.70 };
        let seed_strength =
            edge.convergence.max(edge.strength * 0.35) * (1.0 - 0.35 * edge.obliquity);
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

pub(super) fn classify_convergent_edge(
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
        return (
            Some(ConvergentMode::OceanContinent),
            SubductionPolarity::AUnderB,
        );
    }
    if !a_ocean && b_ocean {
        return (
            Some(ConvergentMode::OceanContinent),
            SubductionPolarity::BUnderA,
        );
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

    (
        Some(ConvergentMode::ContinentContinent),
        SubductionPolarity::None,
    )
}

pub(super) fn estimate_subduction_angle_proxy(
    edge: BoundaryEdge,
    polarity: SubductionPolarity,
    attributes: &[PlateAttr],
    vertex_lithosphere: &[VertexLithosphere],
) -> f32 {
    let (subducting_vertex, overriding_vertex, subducting_plate, overriding_plate) = match polarity
    {
        SubductionPolarity::AUnderB => (edge.a, edge.b, edge.plate_a, edge.plate_b),
        SubductionPolarity::BUnderA => (edge.b, edge.a, edge.plate_b, edge.plate_a),
        SubductionPolarity::None => return 0.5,
    };

    if subducting_vertex >= vertex_lithosphere.len()
        || overriding_vertex >= vertex_lithosphere.len()
    {
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
        0.34 + 0.28 * sub_age + 0.20 * sub_weight + 0.14 * convergence_component
            - 0.08 * over_resistance
            + ocean_ocean_bonus,
        0.0,
        1.0,
    )
}

pub(super) fn subduction_arc_center(subduction_angle: f32, subduction_gate: f32) -> f32 {
    let dip_deg = lerp(
        SUBDUCTION_DIP_MIN_DEG,
        SUBDUCTION_DIP_MAX_DEG,
        clamp(subduction_angle, 0.0, 1.0),
    );
    let dip_rad = dip_deg.to_radians();
    let target_depth_km = lerp(
        SUBDUCTION_ARC_DEPTH_MIN_KM,
        SUBDUCTION_ARC_DEPTH_MAX_KM,
        clamp(subduction_gate, 0.0, 1.0),
    );
    let offset_km = target_depth_km / dip_rad.tan().max(0.20);
    clamp(offset_km / EARTH_RADIUS_KM, 0.015, 0.070)
}

pub(super) fn subduction_gate_for_edge(
    edge: BoundaryEdge,
    polarity: SubductionPolarity,
    vertex_lithosphere: &[VertexLithosphere],
) -> f32 {
    let subducting_vertex = match polarity {
        SubductionPolarity::AUnderB => edge.a,
        SubductionPolarity::BUnderA => edge.b,
        SubductionPolarity::None => return 0.0,
    };
    let Some(subducting) = vertex_lithosphere.get(subducting_vertex).copied() else {
        return 0.0;
    };
    let age_norm = clamp(subducting.age_norm, 0.0, 1.0);
    let density_proxy = clamp(subducting.weight, 0.0, 1.0);
    clamp(
        0.50 * age_norm + 0.35 * density_proxy + 0.15 * edge.convergence,
        0.0,
        1.0,
    )
}

pub(super) struct ArcSignalInput<'a> {
    pub vertex: usize,
    pub pid: usize,
    pub positions: &'a [[f32; 3]],
    pub nbr_offsets: &'a [u32],
    pub nbrs: &'a [u32],
    pub nearest_edge: &'a [usize],
    pub boundary_edges: &'a [BoundaryEdge],
    pub attributes: &'a [PlateAttr],
    pub vertex_lithosphere: &'a [VertexLithosphere],
    pub params: &'a GeologyParams,
}

pub(super) fn accumulate_multi_edge_arc_signal(input: ArcSignalInput<'_>) -> (f32, f32) {
    let vertex = input.vertex;
    let pid = input.pid;
    let positions = input.positions;
    let nbr_offsets = input.nbr_offsets;
    let nbrs = input.nbrs;
    let nearest_edge = input.nearest_edge;
    let boundary_edges = input.boundary_edges;
    let attributes = input.attributes;
    let vertex_lithosphere = input.vertex_lithosphere;
    let params = input.params;

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
        if !matches!(edge.boundary_type, EdgeReliefType::Convergent) {
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
        if !matches!(
            mode,
            ConvergentMode::OceanContinent | ConvergentMode::OceanOcean
        ) {
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
        let subduction_gate = subduction_gate_for_edge(edge, polarity, vertex_lithosphere);
        let arc_center = subduction_arc_center(subduction_angle, subduction_gate)
            * (0.95 + 0.15 * edge.obliquity);

        let edge_mid = normalize3(add3(positions[edge.a], positions[edge.b]));
        let d_mid = chord_distance(pos_v, edge_mid);
        let d_end =
            chord_distance(pos_v, positions[edge.a]).min(chord_distance(pos_v, positions[edge.b]));
        let d = d_mid.min(d_end * 0.9);
        let arc_width = (arc_center * 0.35)
            .max(params.boundary_arc_width * 0.45)
            .min(params.boundary_arc_width * 1.15);
        let arc_w = ring_weight(d, arc_center, arc_width);
        if arc_w <= 1e-4 {
            continue;
        }

        let dist_scale = (-(d * params.boundary_distance_falloff)).exp();
        let source_weight = edge.convergence.max(edge.strength * 0.35)
            * subduction_gate
            * (1.0 - 0.45 * edge.obliquity);
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

pub(super) fn push_unique_edge_candidate(candidates: &mut Vec<usize>, edge_idx: usize) {
    if edge_idx == usize::MAX {
        return;
    }
    if candidates.contains(&edge_idx) {
        return;
    }
    candidates.push(edge_idx);
}

fn compute_continental_stress_assignment(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    attributes: &[PlateAttr],
    fold_sources: &[IntraplateFoldSource],
) -> (Vec<usize>, Vec<f32>) {
    let vertex_count = positions.len();
    let mut nearest_source = vec![usize::MAX; vertex_count];
    let mut dist = vec![f32::INFINITY; vertex_count];
    let mut heap = BinaryHeap::new();

    for (source_idx, source) in fold_sources.iter().enumerate() {
        for &v in &[source.edge.a, source.edge.b] {
            let pid = plate_id[v].as_usize();
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
            let npid = plate_id[n].as_usize();
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

pub(super) fn compute_boundary_distance_assignment(
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

pub(super) fn band_weight(distance: f32, width: f32, anisotropy: f32) -> f32 {
    let sigma = (width * (1.0 - 0.35 * anisotropy)).max(1e-4);
    (-(distance * distance) / (2.0 * sigma * sigma)).exp()
}

pub(super) fn ring_weight(distance: f32, center: f32, width: f32) -> f32 {
    let sigma = width.max(1e-4);
    let dx = distance - center;
    (-(dx * dx) / (2.0 * sigma * sigma)).exp()
}

pub(super) fn smoothstep01(t: f32) -> f32 {
    let x = clamp(t, 0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

pub(super) fn fract01(v: f32) -> f32 {
    v - v.floor()
}

pub(super) fn trig_hash01(pos: [f32; 3], seed: u32) -> f32 {
    let seedf = seed as f32;
    let s = (pos[0] * 12.9898 + pos[1] * 78.233 + pos[2] * 37.719 + seedf * 0.12345).sin();
    fract01(s * 43_758.547)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plate_attr(is_ocean: bool) -> PlateAttr {
        PlateAttr {
            is_ocean,
            velocity: [0.0; 3],
            drift_axis_primary: [0.0; 3],
            drift_axis_secondary: [0.0; 3],
            drift_mix_axis: [0.0; 3],
            drift_variability: 0.0,
            base_height: 0.0,
            base_weight: 0.0,
        }
    }

    fn boundary_strip() -> (Vec<[f32; 3]>, Vec<u32>, Vec<u32>, Vec<PlateId>) {
        let cell_count = 20;
        let mut positions = Vec::with_capacity(cell_count);
        let mut nbr_offsets = Vec::with_capacity(cell_count + 1);
        let mut nbrs = Vec::with_capacity(cell_count * 2 - 2);
        let mut plate_id = Vec::with_capacity(cell_count);

        for i in 0..cell_count {
            let angle = (i as f32 - 9.5) * 0.03;
            positions.push([angle.cos(), angle.sin(), 0.0]);
            plate_id.push(if i < 10 { PlateId(0) } else { PlateId(1) });
            nbr_offsets.push(nbrs.len() as u32);
            if i > 0 {
                nbrs.push((i - 1) as u32);
            }
            if i + 1 < cell_count {
                nbrs.push((i + 1) as u32);
            }
        }
        nbr_offsets.push(nbrs.len() as u32);

        (positions, nbr_offsets, nbrs, plate_id)
    }

    #[test]
    fn artificial_subduction_strip_places_trench_and_arc_on_opposite_sides() {
        let (positions, nbr_offsets, nbrs, plate_id) = boundary_strip();
        let attributes = vec![plate_attr(true), plate_attr(false)];
        let mut vertex_lithosphere = vec![
            VertexLithosphere {
                age_norm: 0.4,
                weight: 0.4,
                buoyancy: 0.2,
                competence: 0.5,
            };
            positions.len()
        ];
        for lithosphere in &mut vertex_lithosphere[..10] {
            lithosphere.age_norm = 1.0;
            lithosphere.weight = 1.0;
            lithosphere.buoyancy = -0.5;
        }
        let boundary_edges = vec![BoundaryEdge {
            a: 9,
            b: 10,
            plate_a: 0,
            plate_b: 1,
            boundary_type: EdgeReliefType::Convergent,
            strength: 1.0,
            obliquity: 0.0,
            convergence: 1.0,
            divergence: 0.0,
            transform: 0.0,
        }];
        let params = GeologyParams::default();
        let mut height = vec![0.0; positions.len()];

        let fields = apply_boundary_model(
            BoundaryModelInput {
                positions: &positions,
                nbr_offsets: &nbr_offsets,
                nbrs: &nbrs,
                plate_id: &plate_id,
                attributes: &attributes,
                vertex_lithosphere: &vertex_lithosphere,
                boundary_edges: &boundary_edges,
                params: &params,
            },
            &mut height,
        );

        assert!(height[..10].iter().copied().fold(f32::INFINITY, f32::min) < 0.0);
        assert!(
            height[10..]
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max)
                > 0.0
        );
        assert!(fields.debug_trench_strength[..10]
            .iter()
            .any(|value| *value > 0.0));
        assert!(fields.debug_trench_strength[10..]
            .iter()
            .all(|value| *value == 0.0));
        assert!(fields.debug_arc_strength[..10]
            .iter()
            .all(|value| *value == 0.0));
        assert!(fields.debug_arc_strength[10..]
            .iter()
            .any(|value| *value > 0.0));
        assert!(height[0].abs() < height[9].abs());
        assert!(
            height[19].abs()
                < height[10..]
                    .iter()
                    .map(|value| value.abs())
                    .fold(0.0, f32::max)
        );
    }

    #[test]
    fn artificial_oceanic_divergence_strip_is_symmetric_and_ridge_centered() {
        let (positions, nbr_offsets, nbrs, plate_id) = boundary_strip();
        let attributes = vec![plate_attr(true), plate_attr(true)];
        let vertex_lithosphere = vec![
            VertexLithosphere {
                age_norm: 0.5,
                weight: 0.5,
                buoyancy: 0.0,
                competence: 0.5,
            };
            positions.len()
        ];
        let boundary_edges = vec![BoundaryEdge {
            a: 9,
            b: 10,
            plate_a: 0,
            plate_b: 1,
            boundary_type: EdgeReliefType::Divergent,
            strength: 1.0,
            obliquity: 0.0,
            convergence: 0.0,
            divergence: 1.0,
            transform: 0.0,
        }];
        let params = GeologyParams::default();
        let mut height = vec![0.0; positions.len()];

        let _ = apply_boundary_model(
            BoundaryModelInput {
                positions: &positions,
                nbr_offsets: &nbr_offsets,
                nbrs: &nbrs,
                plate_id: &plate_id,
                attributes: &attributes,
                vertex_lithosphere: &vertex_lithosphere,
                boundary_edges: &boundary_edges,
                params: &params,
            },
            &mut height,
        );

        assert!(height[9] > 0.0);
        assert!(height[10] > 0.0);
        for offset in 0..10 {
            assert!((height[9 - offset] - height[10 + offset]).abs() <= 1e-6);
        }
        assert!(height[0] < height[9]);
        assert!(height[19] < height[10]);
    }
}
