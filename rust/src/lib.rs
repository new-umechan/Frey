use std::collections::HashMap;

use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
pub struct MeshOutput {
    positions: Vec<f32>,
    indices: Vec<u32>,
}

#[wasm_bindgen]
pub fn generate_mesh(level: u32) -> Result<JsValue, JsValue> {
    if level > 8 {
        return Err(JsValue::from_str("level must be between 0 and 8"));
    }

    let (positions, indices) = generate_icosphere(level);
    let flattened_positions = positions
        .into_iter()
        .flat_map(|v| [v[0], v[1], v[2]])
        .collect::<Vec<f32>>();

    let output = MeshOutput {
        positions: flattened_positions,
        indices,
    };

    serde_wasm_bindgen::to_value(&output)
        .map_err(|err| JsValue::from_str(&format!("failed to serialize mesh output: {err}")))
}

fn generate_icosphere(level: u32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let phi = (1.0 + 5.0_f32.sqrt()) / 2.0;
    let mut positions = vec![
        [-1.0, phi, 0.0],
        [1.0, phi, 0.0],
        [-1.0, -phi, 0.0],
        [1.0, -phi, 0.0],
        [0.0, -1.0, phi],
        [0.0, 1.0, phi],
        [0.0, -1.0, -phi],
        [0.0, 1.0, -phi],
        [phi, 0.0, -1.0],
        [phi, 0.0, 1.0],
        [-phi, 0.0, -1.0],
        [-phi, 0.0, 1.0],
    ];

    for vertex in &mut positions {
        normalize(vertex);
    }

    let mut indices: Vec<u32> = vec![
        0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11, 1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7,
        6, 7, 1, 8, 3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9, 4, 9, 5, 2, 4, 11, 6, 2, 10,
        8, 6, 7, 9, 8, 1,
    ];

    for _ in 0..level {
        let mut midpoint_cache = HashMap::<(u32, u32), u32>::new();
        let mut subdivided_indices = Vec::with_capacity(indices.len() * 4);

        for triangle in indices.chunks_exact(3) {
            let i0 = triangle[0];
            let i1 = triangle[1];
            let i2 = triangle[2];

            let a = midpoint_index(i0, i1, &mut positions, &mut midpoint_cache);
            let b = midpoint_index(i1, i2, &mut positions, &mut midpoint_cache);
            let c = midpoint_index(i2, i0, &mut positions, &mut midpoint_cache);

            subdivided_indices.extend_from_slice(&[i0, a, c]);
            subdivided_indices.extend_from_slice(&[i1, b, a]);
            subdivided_indices.extend_from_slice(&[i2, c, b]);
            subdivided_indices.extend_from_slice(&[a, b, c]);
        }

        indices = subdivided_indices;
    }

    (positions, indices)
}

fn midpoint_index(
    i0: u32,
    i1: u32,
    positions: &mut Vec<[f32; 3]>,
    midpoint_cache: &mut HashMap<(u32, u32), u32>,
) -> u32 {
    let key = if i0 < i1 { (i0, i1) } else { (i1, i0) };
    if let Some(index) = midpoint_cache.get(&key) {
        return *index;
    }

    let v0 = positions[i0 as usize];
    let v1 = positions[i1 as usize];

    let mut midpoint = [
        (v0[0] + v1[0]) * 0.5,
        (v0[1] + v1[1]) * 0.5,
        (v0[2] + v1[2]) * 0.5,
    ];
    normalize(&mut midpoint);

    let index = positions.len() as u32;
    positions.push(midpoint);
    midpoint_cache.insert(key, index);
    index
}

fn normalize(v: &mut [f32; 3]) {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length > 0.0 {
        v[0] /= length;
        v[1] /= length;
        v[2] /= length;
    }
}

#[cfg(test)]
mod tests {
    use super::generate_icosphere;

    #[test]
    fn level_zero_has_expected_topology() {
        let (positions, indices) = generate_icosphere(0);
        assert_eq!(positions.len(), 12);
        assert_eq!(indices.len(), 60);
    }

    #[test]
    fn level_six_has_expected_counts() {
        let (positions, indices) = generate_icosphere(6);
        let expected_faces = 20 * 4_u32.pow(6);
        let expected_vertices = 10 * 4_u32.pow(6) + 2;
        assert_eq!(indices.len() as u32, expected_faces * 3);
        assert_eq!(positions.len() as u32, expected_vertices);
    }
}
