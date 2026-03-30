use super::*;

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
            let vel_a = local_plate_velocity(&attributes[plate_a], plate_a, positions[i]);
            let vel_b = local_plate_velocity(&attributes[plate_b], plate_b, positions[j]);
            let rel_v = sub3(vel_b, vel_a);
            let v_rel_n = dot3(rel_v, edge_dir);
            let v_rel_t_vec = sub3(rel_v, mul3(edge_dir, v_rel_n));
            let v_rel_t = length3(v_rel_t_vec);
            let obliquity = v_rel_t / (v_rel_t + v_rel_n.abs() + 1e-5);
            let (boundary_type, strength) = if v_rel_n > classify_eps {
                (
                    EdgeReliefType::Convergent,
                    clamp((v_rel_n - classify_eps) / 0.25, 0.0, 1.0),
                )
            } else if v_rel_n < -classify_eps {
                (
                    EdgeReliefType::Divergent,
                    clamp((-v_rel_n - classify_eps) / 0.25, 0.0, 1.0),
                )
            } else {
                (
                    EdgeReliefType::Transform,
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
    params: &GeologyParams,
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
        let arc_center = params.boundary_arc_width * arc_offset_scale * (0.9 + 0.4 * edge.obliquity);

        let edge_mid = normalize3(add3(positions[edge.a], positions[edge.b]));
        let d_mid = chord_distance(pos_v, edge_mid);
        let d_end =
            chord_distance(pos_v, positions[edge.a]).min(chord_distance(pos_v, positions[edge.b]));
        let d = d_mid.min(d_end * 0.9);
        let arc_w = ring_weight(d, arc_center, params.boundary_arc_width * 0.70);
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

