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
