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
        };
    }

    let (nearest_edge, boundary_dist, boundary_vertices) =
        compute_boundary_distance_assignment(positions, nbr_offsets, nbrs, &boundary_edges, height.len());

    let mut delta = vec![0.0_f32; height.len()];
    let mut preserve_strength = vec![0.0_f32; height.len()];

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

                            if pid == subducting {
                                let trench_w = band_weight(
                                    d,
                                    params.boundary_width_trench * (0.9 + 0.35 * edge.obliquity),
                                    params.boundary_anisotropy,
                                );
                                let trench = conv_base * params.trench_gain * trench_w;
                                delta[v] -= trench;
                                let outer_rise = ring_weight(
                                    d,
                                    params.boundary_width_trench * 1.6,
                                    params.boundary_width_trench * 0.65,
                                );
                                delta[v] += 0.12 * conv_base * outer_rise * dist_scale;
                                preserve_strength[v] = preserve_strength[v].max(0.95 * trench_w);
                            } else if pid == overriding {
                                let forearc_w = band_weight(
                                    d,
                                    params.boundary_width_trench * 1.35,
                                    params.boundary_anisotropy * 0.6,
                                );
                                delta[v] -= 0.08 * conv_base * forearc_w;

                                let arc_center =
                                    params.boundary_width_arc * (0.9 + 0.4 * edge.obliquity);
                                let arc_w = ring_weight(
                                    d,
                                    arc_center,
                                    params.boundary_width_arc * 0.55,
                                );
                                let arc_gain = if matches!(mode, ConvergentMode::OceanOcean) {
                                    params.arc_gain * 1.15
                                } else {
                                    params.arc_gain
                                };
                                delta[v] += conv_base * arc_gain * arc_w * dist_scale;
                                preserve_strength[v] =
                                    preserve_strength[v].max(0.85 * forearc_w.max(arc_w));
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

    BoundaryFields { preserve_strength }
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
