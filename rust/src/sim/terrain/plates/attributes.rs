use super::*;

fn assign_plate_attributes(
    plate_id: &[u32],
    plate_count: usize,
    phi: &[f32],
    rng: &mut DeterministicRng,
    ocean_plate_ratio: f32,
) -> Vec<PlateAttr> {
    let mut plate_counts = vec![0usize; plate_count];
    let mut plate_phi_sum = vec![0.0f32; plate_count];
    for (v, &pid_u32) in plate_id.iter().enumerate() {
        let pid = pid_u32 as usize;
        if pid >= plate_count {
            continue;
        }
        plate_counts[pid] += 1;
        plate_phi_sum[pid] += phi[v];
    }

    let mut plate_scores = Vec::with_capacity(plate_count);
    for pid in 0..plate_count {
        let mean_phi = if plate_counts[pid] > 0 {
            plate_phi_sum[pid] / plate_counts[pid] as f32
        } else {
            0.0
        };
        let jitter = rng.gen_range_f32(-0.12, 0.12);
        plate_scores.push((pid, mean_phi + jitter, mean_phi));
    }
    plate_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

    let mut ocean_target = ((plate_count as f32) * ocean_plate_ratio).round() as usize;
    if plate_count >= 2 {
        ocean_target = ocean_target.clamp(1, plate_count - 1);
    } else {
        ocean_target = ocean_target.min(plate_count);
    }
    let continent_target = plate_count.saturating_sub(ocean_target);
    let mut is_ocean_plate = vec![true; plate_count];
    for (rank, (pid, _, _)) in plate_scores.iter().enumerate() {
        is_ocean_plate[*pid] = rank >= continent_target;
    }

    let mut attrs = Vec::with_capacity(plate_count);

    for pid in 0..plate_count {
        let is_ocean = is_ocean_plate[pid];
        let dir = rng.gen_range_f32(0.0, 2.0 * std::f32::consts::PI);
        let speed = rng.gen_range_f32(0.3, 1.0);
        let velocity = [speed * dir.cos(), speed * dir.sin(), 0.0];
        let drift_axis_primary = random_unit_vector3(rng);
        let drift_axis_secondary = random_unit_vector3(rng);
        let drift_mix_axis = random_unit_vector3(rng);
        let drift_variability = rng.gen_range_f32(0.06, 0.32);
        let mean_phi = if plate_counts[pid] > 0 {
            plate_phi_sum[pid] / plate_counts[pid] as f32
        } else {
            0.0
        };

        let base_height = if is_ocean {
            clamp(
                -0.09 + 0.02 * mean_phi + rng.gen_range_f32(-0.03, 0.02),
                -0.20,
                -0.02,
            )
        } else {
            clamp(
                0.12 + 0.05 * mean_phi + rng.gen_range_f32(-0.05, 0.06),
                0.03,
                0.30,
            )
        };
        let base_weight = if is_ocean {
            0.62 + rng.gen_range_f32(-0.06, 0.08)
        } else {
            0.22 + rng.gen_range_f32(-0.04, 0.04)
        };

        attrs.push(PlateAttr {
            is_ocean,
            velocity,
            drift_axis_primary,
            drift_axis_secondary,
            drift_mix_axis,
            drift_variability,
            base_height,
            base_weight,
        });
    }

    attrs
}

fn compute_vertex_lithosphere(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    attributes: &[PlateAttr],
    boundary_edges: &[BoundaryEdge],
    params: &TerrainParams,
) -> Vec<VertexLithosphere> {
    const AGE_SPEED_REF: f32 = 0.65;
    const AGE_DIRECTIONAL_INFLUENCE: f32 = 0.35;

    let v_count = positions.len();
    let mut crust_age_dist = vec![f32::INFINITY; v_count];
    let mut lith = vec![
        VertexLithosphere {
            age_norm: 0.0,
            weight: 0.0,
            buoyancy: 0.0,
            competence: 0.5,
        };
        v_count
    ];
    let mut heap = BinaryHeap::new();
    let plate_count = attributes.len();
    let mut plate_age_distance_weight = vec![1.0_f32; plate_count];

    // プレートの速度に応じて重み付け
    for (pid, attr) in attributes.iter().enumerate() {
        let speed = length3(attr.velocity).max(1e-4);
        plate_age_distance_weight[pid] = AGE_SPEED_REF / speed;
    }

    let mut has_divergent_source = vec![false; plate_count];
    let mut has_boundary_seed = vec![false; plate_count];

    for i in 0..v_count {
        let pid = plate_id[i] as usize;
        lith[i].weight = attributes[pid].base_weight;
        lith[i].buoyancy = attributes[pid].base_height;
        lith[i].competence = 0.5;
    }

    let mut continental_competence_raw = vec![0.0_f32; v_count];
    for v in 0..v_count {
        let pid = plate_id[v] as usize;
        if attributes[pid].is_ocean {
            continue;
        }
        continental_competence_raw[v] = sample_continental_competence_noise(
            positions[v],
            pid as u32,
            params.continent_competence_large_scale,
            params.continent_competence_mid_scale,
        );
    }
    smooth_continental_field_by_plate(
        nbr_offsets,
        nbrs,
        plate_id,
        attributes,
        &mut continental_competence_raw,
        3,
    );

    for edge in boundary_edges {
        let is_divergent = matches!(edge.boundary_type, BoundaryType::Divergent);
        for &v in &[edge.a, edge.b] {
            let pv = plate_id[v] as usize;
            if !attributes[pv].is_ocean {
                continue;
            }
            has_boundary_seed[pv] = true;
            if is_divergent {
                has_divergent_source[pv] = true;
                if crust_age_dist[v] > 0.0 {
                    crust_age_dist[v] = 0.0;
                    heap.push(BoundaryDistState {
                        cost: 0.0,
                        vertex: v,
                        source_edge: v,
                    });
                }
            }
        }
    }

    for i in 0..v_count {
        let pid = plate_id[i] as usize;
        if !attributes[pid].is_ocean {
            continue;
        }
        if has_divergent_source[pid] {
            continue;
        }
        if has_boundary_seed[pid] && crust_age_dist[i].is_infinite() {
            let start = nbr_offsets[i] as usize;
            let end = nbr_offsets[i + 1] as usize;
            let is_boundary = nbrs[start..end]
                .iter()
                .any(|&n| plate_id[n as usize] != plate_id[i]);
            if is_boundary {
                crust_age_dist[i] = 0.0;
                heap.push(BoundaryDistState {
                    cost: 0.0,
                    vertex: i,
                    source_edge: i,
                });
            }
        }
    }

    while let Some(state) = heap.pop() {
        if state.cost > crust_age_dist[state.vertex] + 1e-6 {
            continue;
        }
        let pid = plate_id[state.vertex] as usize;
        if !attributes[pid].is_ocean {
            continue;
        }

        let start = nbr_offsets[state.vertex] as usize;
        let end = nbr_offsets[state.vertex + 1] as usize;
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if plate_id[n] != plate_id[state.vertex] {
                continue;
            }
            let npid = plate_id[n] as usize;
            if !attributes[npid].is_ocean {
                continue;
            }
            let step = chord_distance(positions[state.vertex], positions[n]).max(1e-4);
            let base_weight = plate_age_distance_weight[pid];

            let edge_vec = sub3(positions[n], positions[state.vertex]);
            let edge_tangent = project_to_tangent(edge_vec, positions[state.vertex]);
            let edge_dir = normalize3(edge_tangent);

            let plate_vel_tangent =
                local_plate_velocity(&attributes[pid], pid, positions[state.vertex]);
            let plate_vel_dir = normalize3(plate_vel_tangent);
            let dir_alignment = dot3(edge_dir, plate_vel_dir);
            let dir_weight = if length3(plate_vel_tangent) <= 1e-6 {
                1.0
            } else {
                clamp(1.0 - AGE_DIRECTIONAL_INFLUENCE * dir_alignment, 0.55, 1.45)
            };

            let weighted_step = step * base_weight * dir_weight;
            let next_cost = state.cost + weighted_step;
            if next_cost + 1e-6 < crust_age_dist[n] {
                crust_age_dist[n] = next_cost;
                heap.push(BoundaryDistState {
                    cost: next_cost,
                    vertex: n,
                    source_edge: state.source_edge,
                });
            }
        }
    }

    let mut ocean_plate_max_age = vec![0.0_f32; plate_count];
    for v in 0..v_count {
        let pid = plate_id[v] as usize;
        if !attributes[pid].is_ocean {
            continue;
        }
        if crust_age_dist[v].is_finite() {
            ocean_plate_max_age[pid] = ocean_plate_max_age[pid].max(crust_age_dist[v]);
        }
    }

    for v in 0..v_count {
        let pid = plate_id[v] as usize;
        if !attributes[pid].is_ocean {
            lith[v].age_norm = 0.0;
            let competence = clamp(
                0.5 + params.continent_competence_noise_gain * continental_competence_raw[v],
                0.0,
                1.0,
            );
            let weight = attributes[pid].base_weight
                + params.continent_competence_weight_gain * (competence - 0.5);
            lith[v].weight = weight;
            lith[v].buoyancy = attributes[pid].base_height;
            lith[v].competence = competence;
            continue;
        }
        let max_age = ocean_plate_max_age[pid].max(1e-4);
        let age = if crust_age_dist[v].is_finite() {
            clamp(crust_age_dist[v] / max_age, 0.0, 1.0)
        } else {
            0.0
        };
        let weight = attributes[pid].base_weight + 0.42 * age;
        // 海洋の標高は「海嶺で軽く高い → 老化で重く低い」を浮力で一元表現する。
        let buoyancy = (-0.08 + 0.06 * (1.0 - age)) - 0.26 * (weight - 0.62);
        lith[v] = VertexLithosphere {
            age_norm: age,
            weight,
            buoyancy,
            competence: 0.5,
        };
    }

    lith
}

fn sample_continental_competence_noise(
    pos: [f32; 3],
    plate_seed: u32,
    large_scale: f32,
    mid_scale: f32,
) -> f32 {
    let axis_a = seeded_unit_vec(plate_seed ^ 0x85eb_ca6b);
    let axis_b = seeded_unit_vec(plate_seed ^ 0xc2b2_ae35);
    let axis_c = seeded_unit_vec(plate_seed ^ 0x27d4_eb2f);
    let phase_a = std::f32::consts::TAU * hash01_u32(plate_seed ^ 0x517c_c1b7);
    let phase_b = std::f32::consts::TAU * hash01_u32(plate_seed ^ 0x9e37_79b9);
    let phase_c = std::f32::consts::TAU * hash01_u32(plate_seed ^ 0x94d0_49bb);

    let large = (dot3(pos, axis_a) * large_scale + phase_a).sin();
    let mid_primary = (dot3(pos, axis_b) * mid_scale + phase_b).sin();
    let mid_secondary = (dot3(pos, axis_c) * (mid_scale * 1.37) + phase_c).sin();
    let mixed = 0.70 * large + 0.20 * mid_primary + 0.10 * mid_secondary;
    clamp(mixed, -1.0, 1.0)
}

fn smooth_continental_field_by_plate(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    attributes: &[PlateAttr],
    field: &mut [f32],
    iter: u32,
) {
    if iter == 0 || field.is_empty() {
        return;
    }
    let mut buf = field.to_vec();
    for _ in 0..iter {
        for v in 0..field.len() {
            let pid = plate_id[v] as usize;
            if attributes[pid].is_ocean {
                buf[v] = field[v];
                continue;
            }
            let mut sum = field[v];
            let mut wsum = 1.0_f32;
            let start = nbr_offsets[v] as usize;
            let end = nbr_offsets[v + 1] as usize;
            for &n_u32 in &nbrs[start..end] {
                let n = n_u32 as usize;
                if plate_id[n] != plate_id[v] {
                    continue;
                }
                sum += field[n];
                wsum += 1.0;
            }
            buf[v] = sum / wsum;
        }
        field.copy_from_slice(&buf);
    }
}

fn hash01_u32(seed: u32) -> f32 {
    let s = ((seed as f32) * 12.9898 + 78.233).sin();
    fract01(s * 43_758.547)
}

fn seeded_unit_vec(seed: u32) -> [f32; 3] {
    let z = 2.0 * hash01_u32(seed ^ 0x68bc_21eb) - 1.0;
    let phi = std::f32::consts::TAU * hash01_u32(seed ^ 0x02e5_be93);
    let r = (1.0 - z * z).max(0.0).sqrt();
    [r * phi.cos(), z, r * phi.sin()]
}
