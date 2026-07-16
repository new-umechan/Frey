use crate::sim::exec::math::{cross3, dot};
use crate::sim::geology_types::{CrustType, PlateId};

use super::surface_material_transport::{
    nearest_mesh_cell, SurfaceCellMaterialSample, SurfaceMaterialParcel,
};

const MASS_EPSILON: f32 = 1e-8;
const VERTEX_MATCH_EPSILON: f32 = 1e-6;
const TRIANGLE_CONTAINMENT_TOLERANCE: f32 = 2e-3;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct ProjectedPlateMaterial {
    pub plate_id: PlateId,
    pub mass: f32,
    pub oceanic_mass: f32,
    pub age_mass: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct SurfaceProjectionDiagnostics {
    pub input_mass: f32,
    pub projected_mass: f32,
    pub mass_conservation_error: f32,
    pub fallback_parcel_count: u32,
    pub uncovered_cell_count: u32,
    pub mixed_plate_cell_count: u32,
    pub min_cell_mass: f32,
    pub max_cell_mass: f32,
    pub mean_abs_cell_mass_error: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SurfaceMaterialProjection {
    pub cells: Vec<Vec<ProjectedPlateMaterial>>,
    pub diagnostics: SurfaceProjectionDiagnostics,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct ProjectedSurfaceReconstruction {
    pub cells: Vec<Option<SurfaceCellMaterialSample>>,
    pub unresolved_cell_count: u32,
}

pub(super) fn project_surface_material(
    parcels: &mut [SurfaceMaterialParcel],
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> SurfaceMaterialProjection {
    let mut projection = SurfaceMaterialProjection {
        cells: vec![Vec::new(); positions.len()],
        ..Default::default()
    };
    for parcel in parcels {
        projection.diagnostics.input_mass += parcel.mass;
        let Some(stencil) = projection_stencil(
            parcel.position,
            parcel.host_cell as usize,
            positions,
            nbr_offsets,
            nbrs,
        ) else {
            projection.diagnostics.fallback_parcel_count = projection
                .diagnostics
                .fallback_parcel_count
                .saturating_add(1);
            continue;
        };
        parcel.host_cell = stencil.host_cell as u32;
        if stencil.used_fallback {
            projection.diagnostics.fallback_parcel_count = projection
                .diagnostics
                .fallback_parcel_count
                .saturating_add(1);
        }
        for contribution in stencil.contributions() {
            let mass = parcel.mass * contribution.weight;
            deposit_parcel_mass(&mut projection.cells[contribution.cell], *parcel, mass);
            projection.diagnostics.projected_mass += mass;
        }
    }
    finish_projection_diagnostics(&mut projection);
    projection
}

pub(super) fn reconstruct_projected_surface(
    projection: &SurfaceMaterialProjection,
) -> ProjectedSurfaceReconstruction {
    let mut reconstruction = ProjectedSurfaceReconstruction {
        cells: Vec::with_capacity(projection.cells.len()),
        ..Default::default()
    };
    for materials in &projection.cells {
        let selected = materials.iter().max_by(|a, b| {
            a.mass
                .total_cmp(&b.mass)
                .then_with(|| b.plate_id.cmp(&a.plate_id))
        });
        let sample = selected.map(|material| SurfaceCellMaterialSample {
            plate_id: material.plate_id,
            crust_type: if material.oceanic_mass * 2.0 >= material.mass {
                CrustType::Oceanic
            } else {
                CrustType::Continental
            },
            crust_age: material.age_mass / material.mass.max(MASS_EPSILON),
            mass: material.mass,
        });
        if sample.is_none() {
            reconstruction.unresolved_cell_count =
                reconstruction.unresolved_cell_count.saturating_add(1);
        }
        reconstruction.cells.push(sample);
    }
    reconstruction
}

#[derive(Clone, Copy)]
struct ProjectionContribution {
    cell: usize,
    weight: f32,
}

#[derive(Clone, Copy)]
struct ProjectionStencil {
    host_cell: usize,
    cells: [usize; 3],
    weights: [f32; 3],
    count: usize,
    used_fallback: bool,
}

impl ProjectionStencil {
    fn contributions(self) -> impl Iterator<Item = ProjectionContribution> {
        (0..self.count).map(move |index| ProjectionContribution {
            cell: self.cells[index],
            weight: self.weights[index],
        })
    }
}

fn projection_stencil(
    position: [f32; 3],
    start_cell: usize,
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> Option<ProjectionStencil> {
    let host = nearest_mesh_cell(position, start_cell, positions, nbr_offsets, nbrs)?;
    if 1.0 - dot(position, positions[host]) <= VERTEX_MATCH_EPSILON {
        return Some(single_cell_stencil(host, false));
    }
    if let Some(stencil) = containing_triangle_stencil(position, host, positions, nbr_offsets, nbrs)
    {
        return Some(stencil);
    }
    Some(single_cell_stencil(host, true))
}

fn containing_triangle_stencil(
    position: [f32; 3],
    host: usize,
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> Option<ProjectionStencil> {
    let neighbors = cell_neighbors(host, nbr_offsets, nbrs)?;
    let mut best = None::<(f32, ProjectionStencil)>;
    for left_index in 0..neighbors.len() {
        for right_index in left_index + 1..neighbors.len() {
            let left = neighbors[left_index] as usize;
            let right = neighbors[right_index] as usize;
            if left >= positions.len()
                || right >= positions.len()
                || !cells_are_neighbors(left, right, nbr_offsets, nbrs)
            {
                continue;
            }
            let Some((weights, error)) = spherical_triangle_weights(
                position,
                positions[host],
                positions[left],
                positions[right],
            ) else {
                continue;
            };
            let stencil = ProjectionStencil {
                host_cell: host,
                cells: [host, left, right],
                weights,
                count: 3,
                used_fallback: false,
            };
            if best.is_none_or(|(best_error, _)| error < best_error) {
                best = Some((error, stencil));
            }
        }
    }
    best.filter(|(error, _)| *error <= TRIANGLE_CONTAINMENT_TOLERANCE)
        .map(|(_, stencil)| stencil)
}

fn spherical_triangle_weights(
    point: [f32; 3],
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
) -> Option<([f32; 3], f32)> {
    let total_area = spherical_triangle_area(a, b, c);
    if !total_area.is_finite() || total_area <= MASS_EPSILON {
        return None;
    }
    let raw = [
        spherical_triangle_area(point, b, c) / total_area,
        spherical_triangle_area(point, c, a) / total_area,
        spherical_triangle_area(point, a, b) / total_area,
    ];
    let sum = raw.iter().copied().sum::<f32>();
    if !sum.is_finite() || raw.iter().any(|weight| !weight.is_finite()) {
        return None;
    }
    let error = (sum - 1.0).abs();
    let denominator = sum.max(MASS_EPSILON);
    Some((raw.map(|weight| weight.max(0.0) / denominator), error))
}

fn spherical_triangle_area(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    let numerator = dot(a, cross3(b, c)).abs();
    let denominator = 1.0 + dot(a, b) + dot(b, c) + dot(c, a);
    2.0 * numerator.atan2(denominator.max(MASS_EPSILON))
}

fn single_cell_stencil(cell: usize, used_fallback: bool) -> ProjectionStencil {
    ProjectionStencil {
        host_cell: cell,
        cells: [cell, cell, cell],
        weights: [1.0, 0.0, 0.0],
        count: 1,
        used_fallback,
    }
}

fn cell_neighbors<'a>(cell: usize, nbr_offsets: &[u32], nbrs: &'a [u32]) -> Option<&'a [u32]> {
    let start = *nbr_offsets.get(cell)? as usize;
    let end = *nbr_offsets.get(cell + 1)? as usize;
    nbrs.get(start..end)
}

fn cells_are_neighbors(a: usize, b: usize, nbr_offsets: &[u32], nbrs: &[u32]) -> bool {
    cell_neighbors(a, nbr_offsets, nbrs)
        .is_some_and(|neighbors| neighbors.iter().any(|&cell| cell as usize == b))
}

fn deposit_parcel_mass(
    materials: &mut Vec<ProjectedPlateMaterial>,
    parcel: SurfaceMaterialParcel,
    mass: f32,
) {
    deposit_projected_material(
        materials,
        parcel.plate_id,
        parcel.crust_type,
        parcel.crust_age,
        mass,
    );
}

pub(super) fn deposit_projected_material(
    materials: &mut Vec<ProjectedPlateMaterial>,
    plate_id: PlateId,
    crust_type: CrustType,
    crust_age: f32,
    mass: f32,
) {
    if mass <= MASS_EPSILON {
        return;
    }
    deposit_projected_mass_components(
        materials,
        plate_id,
        mass,
        if crust_type == CrustType::Oceanic {
            mass
        } else {
            0.0
        },
        crust_age * mass,
    );
}

pub(super) fn deposit_projected_mass_components(
    materials: &mut Vec<ProjectedPlateMaterial>,
    plate_id: PlateId,
    mass: f32,
    oceanic_mass: f32,
    age_mass: f32,
) {
    if mass <= MASS_EPSILON {
        return;
    }
    let index = materials
        .iter()
        .position(|material| material.plate_id == plate_id)
        .unwrap_or_else(|| {
            materials.push(ProjectedPlateMaterial {
                plate_id,
                ..Default::default()
            });
            materials.len() - 1
        });
    let material = &mut materials[index];
    material.mass += mass;
    material.age_mass += age_mass;
    material.oceanic_mass += oceanic_mass.clamp(0.0, mass);
}

pub(super) fn finish_projection_diagnostics(projection: &mut SurfaceMaterialProjection) {
    projection.diagnostics.uncovered_cell_count = 0;
    projection.diagnostics.mixed_plate_cell_count = 0;
    projection.diagnostics.min_cell_mass = 0.0;
    projection.diagnostics.max_cell_mass = 0.0;
    projection.diagnostics.mean_abs_cell_mass_error = 0.0;
    let mut min_cell_mass = f32::INFINITY;
    let mut abs_mass_error_sum = 0.0_f32;
    for materials in &projection.cells {
        let cell_mass = materials.iter().map(|material| material.mass).sum::<f32>();
        min_cell_mass = min_cell_mass.min(cell_mass);
        projection.diagnostics.max_cell_mass = projection.diagnostics.max_cell_mass.max(cell_mass);
        abs_mass_error_sum += (cell_mass - 1.0).abs();
        if cell_mass <= MASS_EPSILON {
            projection.diagnostics.uncovered_cell_count = projection
                .diagnostics
                .uncovered_cell_count
                .saturating_add(1);
        }
        if materials
            .iter()
            .filter(|material| material.mass > MASS_EPSILON)
            .count()
            > 1
        {
            projection.diagnostics.mixed_plate_cell_count = projection
                .diagnostics
                .mixed_plate_cell_count
                .saturating_add(1);
        }
    }
    projection.diagnostics.min_cell_mass = if min_cell_mass.is_finite() {
        min_cell_mass
    } else {
        0.0
    };
    projection.diagnostics.mean_abs_cell_mass_error = if projection.cells.is_empty() {
        0.0
    } else {
        abs_mass_error_sum / projection.cells.len() as f32
    };
    projection.diagnostics.mass_conservation_error =
        (projection.diagnostics.projected_mass - projection.diagnostics.input_mass).abs();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::mesh::{build_neighbors, generate_icosphere};
    use crate::sim::world::{PlateKinematicsState, VertexCrustState};

    use super::super::surface_material_transport::{
        quadrature_parcels_from_mesh, transport_surface_material,
    };

    fn parcel(position: [f32; 3], host_cell: u32) -> SurfaceMaterialParcel {
        SurfaceMaterialParcel {
            position,
            host_cell,
            plate_id: PlateId(0),
            crust_type: CrustType::Oceanic,
            crust_age: 24.0,
            mass: 1.0,
        }
    }

    fn plate_state() -> PlateKinematicsState {
        PlateKinematicsState {
            angular_axis: [0.3, 0.8, -0.2],
            angular_speed: 0.08,
            reference_angular_speed: 0.08,
            slab_pull_drive: 0.0,
            ridge_push_drive: 0.0,
            collision_drag: 0.0,
            force_target_speed_km_per_myr: 0.0,
            basal_target_speed_km_per_myr: 0.0,
            phase_offset: 0.0,
            activity: 0.0,
        }
    }

    fn oceanic_crust() -> VertexCrustState {
        VertexCrustState {
            crust_type: CrustType::Oceanic,
            thickness: 1.0,
            density: 3_000.0,
            age: 24.0,
            stress: 0.0,
            temperature: 0.0,
            rigidity: 1.0,
            arc_volcanism: 0.0,
            ridge_volcanism: 0.0,
            hotspot_volcanism: 0.0,
            backarc_volcanism: 0.0,
            stress_tensor: Default::default(),
        }
    }

    #[test]
    fn vertex_aligned_projection_is_one_to_one_and_conservative() {
        let (positions, indices) = generate_icosphere(1);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let mut parcels = positions
            .iter()
            .copied()
            .enumerate()
            .map(|(cell, position)| parcel(position, cell as u32))
            .collect::<Vec<_>>();

        let projection = project_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);

        assert_eq!(projection.diagnostics.uncovered_cell_count, 0);
        assert_eq!(projection.diagnostics.fallback_parcel_count, 0);
        assert!(projection.diagnostics.mass_conservation_error < 1e-5);
        assert_eq!(projection.diagnostics.min_cell_mass, 1.0);
        assert_eq!(projection.diagnostics.max_cell_mass, 1.0);
    }

    #[test]
    fn arbitrary_rigid_rotation_projects_without_holes_or_mass_loss() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let mut parcels = positions
            .iter()
            .copied()
            .enumerate()
            .map(|(cell, position)| parcel(position, cell as u32))
            .collect::<Vec<_>>();
        transport_surface_material(&mut parcels, &[plate_state()]);

        let projection = project_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);
        let reconstruction = reconstruct_projected_surface(&projection);

        assert_eq!(projection.diagnostics.fallback_parcel_count, 0);
        assert_eq!(projection.diagnostics.uncovered_cell_count, 0);
        assert!(projection.diagnostics.mass_conservation_error < 1e-4);
        assert_eq!(reconstruction.unresolved_cell_count, 0);
    }

    #[test]
    fn triangle_center_distributes_mass_to_three_vertices() {
        let (positions, indices) = generate_icosphere(1);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let triangle = [
            indices[0] as usize,
            indices[1] as usize,
            indices[2] as usize,
        ];
        let sum = [
            positions[triangle[0]][0] + positions[triangle[1]][0] + positions[triangle[2]][0],
            positions[triangle[0]][1] + positions[triangle[1]][1] + positions[triangle[2]][1],
            positions[triangle[0]][2] + positions[triangle[1]][2] + positions[triangle[2]][2],
        ];
        let length = (dot(sum, sum)).sqrt();
        let center = [sum[0] / length, sum[1] / length, sum[2] / length];
        let mut parcels = vec![parcel(center, triangle[0] as u32)];

        let projection = project_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);
        let occupied = projection
            .cells
            .iter()
            .filter(|materials| !materials.is_empty())
            .count();

        assert_eq!(projection.diagnostics.fallback_parcel_count, 0);
        assert_eq!(occupied, 3);
        assert!(projection.diagnostics.mass_conservation_error < 1e-6);
    }

    #[test]
    fn quadrature_projection_preserves_full_sphere_under_rigid_rotation() {
        let (positions, indices) = generate_icosphere(6);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let plate_id = vec![PlateId(0); positions.len()];
        let crust = vec![oceanic_crust(); positions.len()];
        let mut parcels =
            quadrature_parcels_from_mesh(&positions, &nbr_offsets, &nbrs, &plate_id, &crust)
                .unwrap();
        transport_surface_material(&mut parcels, &[plate_state()]);

        let projection = project_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);

        assert_eq!(projection.diagnostics.fallback_parcel_count, 0);
        assert_eq!(projection.diagnostics.uncovered_cell_count, 0);
        assert!(
            projection.diagnostics.mass_conservation_error
                <= projection.diagnostics.input_mass * 1e-4
        );
    }
}
