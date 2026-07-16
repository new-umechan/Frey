use crate::sim::exec::math::dot;
use crate::sim::geology_types::{CrustType, PlateId};
use crate::sim::world::{BoundaryDynamicsState, BoundaryType};
use crate::sim::world::{PlateKinematicsState, SurfaceMaterialElementState, VertexCrustState};

use super::surface_boundary_sweep::SweptBoundaryPlan;
use super::surface_cell_geometry::build_barycentric_dual_cells;
#[cfg(test)]
use super::surface_material_overlap::spherical_polygon_area_vertices;
use super::surface_material_overlap::{
    cut_spherical_polygon_by_area_fraction, polygon_overlap_fractions,
};
use super::surface_material_projection::{ProjectedPlateMaterial, SurfaceMaterialProjection};
use super::surface_material_transport::{nearest_mesh_cell, rotate_unit_vector};

const AREA_EPSILON: f32 = 1e-10;
const MIN_REPRESENTABLE_CELL_FRACTION: f32 = 1e-3;
// Elements below this fraction of a mean mesh cell are numerical dust. Keeping
// them makes the f32 gnomonic projection less stable without preserving useful
// ownership history.
const NUMERICAL_DUST_CELL_FRACTION: f32 = 1e-4;

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SurfaceMaterialElementProjection {
    pub cells: Vec<Vec<ProjectedElementMaterial>>,
    pub target_cell_areas: Vec<f32>,
    pub input_area: f32,
    pub projected_area: f32,
    pub uncovered_area: f32,
    pub overlap_area: f32,
    pub absolute_coverage_error: f32,
    pub unassigned_element_count: u32,
    pub unassigned_element_area: f32,
    pub max_unassigned_element_area: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct ProjectedElementMaterial {
    pub plate_id: PlateId,
    pub area: f32,
    pub oceanic_area: f32,
    pub age_area: f32,
    pub first_moment: [f32; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct MaterialCoverageRegimeDiagnostics {
    pub ridge_gap_area: f32,
    pub unsupported_gap_area: f32,
    pub subduction_overlap_area: f32,
    pub collision_overlap_area: f32,
    pub unsupported_overlap_area: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct MaterialCoverageClosureDiagnostics {
    pub ridge_created_area: f32,
    pub subducted_area: f32,
    pub collision_excess_area: f32,
    pub residual_gap_area: f32,
    pub residual_overlap_area: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct MaterialElementReconstructionDiagnostics {
    pub mixed_cell_count: u32,
    pub quadrature_closure_area: f32,
    pub max_relative_quadrature_closure: f32,
    pub reconstructed_area_error: f32,
    pub first_moment_squared_error: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct SurfaceMarkerOwnershipDiagnostics {
    pub empty_candidate_cell_count: u32,
    pub single_candidate_cell_count: u32,
    pub mixed_candidate_cell_count: u32,
    pub changed_empty_candidate_cell_count: u32,
    pub changed_single_candidate_cell_count: u32,
    pub changed_mixed_candidate_cell_count: u32,
    pub reversed_empty_candidate_cell_count: u32,
    pub reversed_single_candidate_cell_count: u32,
    pub reversed_mixed_candidate_cell_count: u32,
    pub changed_divergent_cell_count: u32,
    pub changed_subduction_cell_count: u32,
    pub changed_collision_cell_count: u32,
    pub changed_transform_cell_count: u32,
}

pub(super) struct SurfaceMaterialElementUpdateInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub nbr_offsets: &'a [u32],
    pub nbrs: &'a [u32],
    pub plate_id: &'a [PlateId],
    pub crust: &'a [VertexCrustState],
    pub plate_states: &'a [PlateKinematicsState],
    pub boundary_state: &'a BoundaryDynamicsState,
    pub elements: &'a mut Vec<SurfaceMaterialElementState>,
}

pub(super) struct SurfaceMaterialElementUpdate {
    pub plate_id: Vec<PlateId>,
    pub crust_type: Vec<CrustType>,
    pub crust_age: Vec<f32>,
    pub closure: MaterialCoverageClosureDiagnostics,
    pub coverage: MaterialCoverageRegimeDiagnostics,
    pub marker_ownership: SurfaceMarkerOwnershipDiagnostics,
}

pub(super) fn update_persistent_surface_material_elements(
    input: SurfaceMaterialElementUpdateInput<'_>,
    apply_reactions: bool,
    previous_previous_plate_id: &[PlateId],
) -> Result<SurfaceMaterialElementUpdate, String> {
    if input.elements.is_empty() {
        *input.elements = initialize_surface_material_elements(
            input.positions,
            input.nbr_offsets,
            input.nbrs,
            input.plate_id,
            input.crust,
        )?;
    }
    advect_surface_material_elements(input.elements, input.plate_states)?;
    discard_subcell_material_dust(input.elements, input.positions.len());
    let mut projection = project_surface_material_elements(
        input.elements,
        input.positions,
        input.nbr_offsets,
        input.nbrs,
    )?;
    if projection.unassigned_element_count > 0 {
        return Err(format!(
            "{} persistent material elements were not projected: area={}, max_area={}",
            projection.unassigned_element_count,
            projection.unassigned_element_area,
            projection.max_unassigned_element_area
        ));
    }
    let bridge_projection = surface_projection_from_elements(&projection);
    let plan = super::surface_boundary_sweep::plan_swept_boundary_reactions(
        super::surface_boundary_sweep::SweptBoundaryInput {
            positions: input.positions,
            nbr_offsets: input.nbr_offsets,
            nbrs: input.nbrs,
            plate_id: input.plate_id,
            crust: input.crust,
            plate_states: input.plate_states,
            boundary_state: input.boundary_state,
            projection: &bridge_projection,
            cell_capacity: Some(&projection.target_cell_areas),
        },
    );
    let mut closure = if apply_reactions {
        apply_persistent_material_reactions(
            input.elements,
            &projection,
            &plan,
            input.positions,
            input.nbr_offsets,
            input.nbrs,
        )?
    } else {
        MaterialCoverageClosureDiagnostics::default()
    };
    discard_subcell_material_dust(input.elements, input.positions.len());
    projection = project_surface_material_elements(
        input.elements,
        input.positions,
        input.nbr_offsets,
        input.nbrs,
    )?;
    let (plate_id, marker_ownership) = rasterize_persistent_material_surface(
        input.elements,
        input.positions,
        input.nbr_offsets,
        input.nbrs,
        input.plate_id,
        previous_previous_plate_id,
        &projection,
        &plan,
    )?;
    projection = project_surface_material_elements(
        input.elements,
        input.positions,
        input.nbr_offsets,
        input.nbrs,
    )?;
    closure.residual_gap_area = projection.uncovered_area;
    closure.residual_overlap_area = projection.overlap_area;
    let coverage = classify_material_coverage_regimes(&projection, input.boundary_state, &plan);
    let mut crust_type = Vec::with_capacity(plate_id.len());
    let mut crust_age = Vec::with_capacity(plate_id.len());
    for (cell, &plate) in plate_id.iter().enumerate() {
        if let Some(material) = projection.cells[cell]
            .iter()
            .find(|material| material.plate_id == plate)
        {
            crust_type.push(if material.oceanic_area * 2.0 >= material.area {
                CrustType::Oceanic
            } else {
                CrustType::Continental
            });
            crust_age.push(material.age_area / material.area.max(AREA_EPSILON));
        } else {
            crust_type.push(CrustType::Oceanic);
            crust_age.push(0.0);
        }
    }
    Ok(SurfaceMaterialElementUpdate {
        plate_id,
        crust_type,
        crust_age,
        closure,
        coverage,
        marker_ownership,
    })
}

fn discard_subcell_material_dust(
    elements: &mut Vec<SurfaceMaterialElementState>,
    cell_count: usize,
) {
    let mean_cell_area = 4.0 * std::f32::consts::PI / cell_count.max(1) as f32;
    let threshold = mean_cell_area * NUMERICAL_DUST_CELL_FRACTION;
    elements.retain(|element| element.area > threshold);
}

fn apply_persistent_material_reactions(
    elements: &mut Vec<SurfaceMaterialElementState>,
    projection: &SurfaceMaterialElementProjection,
    plan: &SweptBoundaryPlan,
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> Result<MaterialCoverageClosureDiagnostics, String> {
    let mut diagnostics = MaterialCoverageClosureDiagnostics::default();
    let mut removal = std::collections::BTreeMap::<(usize, PlateId), f32>::new();
    for assignment in &plan.subduction_cells {
        let cell = assignment.cell as usize;
        let Some(materials) = projection.cells.get(cell) else {
            continue;
        };
        let total = materials.iter().map(|material| material.area).sum::<f32>();
        let subducting_oceanic = materials
            .iter()
            .find(|material| material.plate_id == assignment.subducting_plate)
            .map(|material| material.oceanic_area)
            .unwrap_or(0.0);
        let amount = (total - assignment.target_mass)
            .max(0.0)
            .min(subducting_oceanic);
        if amount > AREA_EPSILON {
            *removal
                .entry((cell, assignment.subducting_plate))
                .or_default() += amount;
        }
    }
    if !removal.is_empty() {
        let old_elements = std::mem::take(elements);
        for element in old_elements {
            let key = (element.host_cell as usize, element.plate_id);
            let requested = removal.get(&key).copied().unwrap_or(0.0);
            let amount = requested.min(element.oceanic_area).min(element.area);
            if amount <= AREA_EPSILON {
                elements.push(element);
                continue;
            }
            diagnostics.subducted_area += amount;
            if let Some(remaining) = removal.get_mut(&key) {
                *remaining = (*remaining - amount).max(0.0);
            }
            let retained_fraction = (element.area - amount) / element.area.max(AREA_EPSILON);
            if retained_fraction <= AREA_EPSILON {
                continue;
            }
            let center = normalized([
                element.vertices[0][0] + element.vertices[1][0] + element.vertices[2][0],
                element.vertices[0][1] + element.vertices[1][1] + element.vertices[2][1],
                element.vertices[0][2] + element.vertices[1][2] + element.vertices[2][2],
            ])
            .ok_or_else(|| "subducted material element has invalid center".to_string())?;
            let toward = element.vertices[0];
            let (support, _) = cut_spherical_polygon_by_area_fraction(
                &element.vertices,
                center,
                toward,
                retained_fraction,
            )
            .ok_or_else(|| "failed to trim subducted material element".to_string())?;
            append_element_polygon(
                elements,
                element.plate_id,
                element.host_cell as usize,
                &support,
                element.oceanic_area / element.area.max(AREA_EPSILON),
                element.age_area / element.area.max(AREA_EPSILON),
                element.ownership_marker,
            )?;
        }
    }

    let dual_cells = build_barycentric_dual_cells(positions, nbr_offsets, nbrs)
        .ok_or_else(|| "failed to build dual cells for ridge material".to_string())?;
    for assignment in &plan.divergent_cells {
        let cell = assignment.cell as usize;
        let Some((&center, polygon, materials, &target_area)) = positions
            .get(cell)
            .zip(dual_cells.get(cell))
            .zip(projection.cells.get(cell))
            .zip(projection.target_cell_areas.get(cell))
            .map(|(((center, polygon), materials), area)| (center, polygon, materials, area))
        else {
            continue;
        };
        let projected_area = materials.iter().map(|material| material.area).sum::<f32>();
        let gap_area = (target_area - projected_area).max(0.0);
        if gap_area <= AREA_EPSILON {
            continue;
        }
        let cell_moment = spherical_polygon_first_moment(polygon)
            .ok_or_else(|| format!("ridge cell {cell} has no first moment"))?;
        let mut gap_moment = cell_moment;
        for material in materials {
            for axis in 0..3 {
                gap_moment[axis] -= material.first_moment[axis];
            }
        }
        let gap_plate = PlateId(u32::MAX);
        let mut phases = materials.clone();
        phases.push(ProjectedElementMaterial {
            plate_id: gap_plate,
            area: gap_area,
            first_moment: gap_moment,
            ..Default::default()
        });
        phases.sort_by_key(|phase| phase.plate_id);
        let partition = reconstruct_multimaterial_mof(polygon, center, &phases)
            .ok_or_else(|| format!("failed to reconstruct ridge gap in cell {cell}"))?;
        let support = partition
            .into_iter()
            .find_map(|(phase, support)| (phase.plate_id == gap_plate).then_some(support))
            .ok_or_else(|| format!("ridge gap phase is missing in cell {cell}"))?;
        append_element_polygon(
            elements,
            assignment.accreting_plate,
            cell,
            &support,
            1.0,
            0.0,
            false,
        )?;
        diagnostics.ridge_created_area += gap_area;
    }
    Ok(diagnostics)
}

fn append_element_polygon(
    elements: &mut Vec<SurfaceMaterialElementState>,
    plate_id: PlateId,
    host_cell: usize,
    polygon: &[[f32; 3]],
    oceanic_fraction: f32,
    mean_age: f32,
    ownership_marker: bool,
) -> Result<(), String> {
    if polygon.len() < 3 {
        return Err("material reaction produced a degenerate polygon".to_string());
    }
    for index in 1..polygon.len() - 1 {
        let vertices = [polygon[0], polygon[index], polygon[index + 1]];
        let area = spherical_triangle_area(vertices);
        if area <= AREA_EPSILON {
            continue;
        }
        elements.push(SurfaceMaterialElementState {
            plate_id,
            vertices,
            area,
            oceanic_area: area * oceanic_fraction.clamp(0.0, 1.0),
            age_area: area * mean_age.max(0.0),
            host_cell: host_cell as u32,
            ownership_marker,
        });
    }
    Ok(())
}

fn rasterize_persistent_material_surface(
    elements: &[SurfaceMaterialElementState],
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    previous: &[PlateId],
    previous_previous: &[PlateId],
    projection: &SurfaceMaterialElementProjection,
    plan: &SweptBoundaryPlan,
) -> Result<(Vec<PlateId>, SurfaceMarkerOwnershipDiagnostics), String> {
    let mut candidates = vec![Vec::<PlateId>::new(); positions.len()];
    for element in elements {
        if !element.ownership_marker {
            continue;
        }
        let host = element.host_cell as usize;
        let mut cells = vec![host];
        if host < positions.len() {
            let begin = nbr_offsets[host] as usize;
            let end = nbr_offsets[host + 1] as usize;
            cells.extend(nbrs[begin..end].iter().map(|&cell| cell as usize));
        }
        for cell in cells {
            let Some(&position) = positions.get(cell) else {
                continue;
            };
            if spherical_triangle_contains(element.vertices, position) {
                candidates[cell].push(element.plate_id);
            }
        }
    }
    let divergent = plan
        .divergent_cells
        .iter()
        .map(|assignment| (assignment.cell as usize, assignment.accreting_plate))
        .collect::<std::collections::BTreeMap<_, _>>();
    let subducting = plan
        .subduction_cells
        .iter()
        .map(|assignment| (assignment.cell as usize, assignment.subducting_plate))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut labels = vec![None; candidates.len()];
    let mut hard_cell_count = std::collections::BTreeMap::<PlateId, usize>::new();
    for (cell, plates) in candidates.iter_mut().enumerate() {
        plates.sort_unstable();
        plates.dedup();
        if let Some(&subducting_plate) = subducting.get(&cell) {
            plates.retain(|plate| *plate != subducting_plate);
        }
    }
    let candidate_counts = candidates
        .iter()
        .map(|plates| plates.len())
        .collect::<Vec<_>>();
    let mut visited = vec![false; candidates.len()];
    let mut cores = std::collections::BTreeMap::<PlateId, Vec<Vec<usize>>>::new();
    for start in 0..candidates.len() {
        if visited[start] || candidates[start].len() != 1 {
            continue;
        }
        let plate = candidates[start][0];
        let mut component = Vec::new();
        let mut stack = vec![start];
        visited[start] = true;
        while let Some(cell) = stack.pop() {
            component.push(cell);
            let begin = nbr_offsets[cell] as usize;
            let end = nbr_offsets[cell + 1] as usize;
            for &neighbor in &nbrs[begin..end] {
                let neighbor = neighbor as usize;
                if !visited[neighbor]
                    && candidates[neighbor].len() == 1
                    && candidates[neighbor][0] == plate
                {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
        cores.entry(plate).or_default().push(component);
    }
    for components in cores.values_mut() {
        components.sort_by_key(|component| std::cmp::Reverse(component.len()));
        for component in components.iter().skip(1) {
            for &cell in component {
                candidates[cell].clear();
            }
        }
    }
    for (cell, plates) in candidates.iter().enumerate() {
        let hard_label = if plates.len() == 1 {
            Some(plates[0])
        } else {
            None
        };
        if let Some(plate) = hard_label {
            labels[cell] = Some(plate);
            *hard_cell_count.entry(plate).or_default() += 1;
        }
    }
    let material_plates = elements
        .iter()
        .map(|element| element.plate_id)
        .collect::<std::collections::BTreeSet<_>>();
    for plate in material_plates {
        if hard_cell_count.get(&plate).copied().unwrap_or(0) > 0 {
            continue;
        }
        let seed = (0..labels.len())
            .filter(|&cell| labels[cell].is_none() && candidates[cell].contains(&plate))
            .filter_map(|cell| {
                let area = projection.cells[cell]
                    .iter()
                    .find(|material| material.plate_id == plate)?
                    .area;
                Some((cell, usize::from(previous[cell] == plate), area))
            })
            .max_by(|a, b| {
                a.1.cmp(&b.1)
                    .then_with(|| a.2.total_cmp(&b.2))
                    .then_with(|| b.0.cmp(&a.0))
            })
            .ok_or_else(|| format!("plate {} has no exposed material seed", plate.0))?;
        labels[seed.0] = Some(plate);
        hard_cell_count.insert(plate, 1);
    }
    let mut unresolved = labels.iter().filter(|label| label.is_none()).count();
    while unresolved > 0 {
        let mut proposals = Vec::new();
        for cell in 0..labels.len() {
            if labels[cell].is_some() {
                continue;
            }
            let begin = nbr_offsets[cell] as usize;
            let end = nbr_offsets[cell + 1] as usize;
            let mut support = std::collections::BTreeMap::<PlateId, usize>::new();
            for &neighbor in &nbrs[begin..end] {
                if let Some(plate) = labels[neighbor as usize] {
                    if candidates[cell].is_empty() || candidates[cell].contains(&plate) {
                        let ridge_weight = usize::from(divergent.get(&cell) == Some(&plate));
                        *support.entry(plate).or_default() += 1 + ridge_weight;
                    }
                }
            }
            if let Some((&plate, &neighbor_count)) = support.iter().max_by(|a, b| {
                a.1.cmp(b.1)
                    .then_with(|| hard_cell_count[a.0].cmp(&hard_cell_count[b.0]))
                    .then_with(|| b.0.cmp(a.0))
            }) {
                let material_area = projection.cells[cell]
                    .iter()
                    .find(|material| material.plate_id == plate)
                    .map(|material| material.area)
                    .unwrap_or(0.0);
                proposals.push((cell, plate, neighbor_count, material_area));
            }
        }
        if proposals.is_empty() {
            let cell = labels
                .iter()
                .position(Option::is_none)
                .ok_or_else(|| "persistent material surface lost an unresolved cell".to_string())?;
            let plate = candidates[cell]
                .iter()
                .copied()
                .max_by_key(|plate| {
                    (
                        hard_cell_count.get(plate).copied().unwrap_or(0),
                        std::cmp::Reverse(*plate),
                    )
                })
                .unwrap_or(previous[cell]);
            labels[cell] = Some(plate);
            unresolved -= 1;
            continue;
        }
        proposals.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then_with(|| b.3.total_cmp(&a.3))
                .then_with(|| a.0.cmp(&b.0))
        });
        for (cell, plate, _, _) in proposals {
            if labels[cell].is_none() {
                labels[cell] = Some(plate);
                unresolved -= 1;
            }
        }
    }
    let plate_id = labels
        .into_iter()
        .enumerate()
        .map(|(cell, label)| {
            label.ok_or_else(|| format!("persistent material surface leaves cell {cell} empty"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let divergent_cells = plan
        .divergent_cells
        .iter()
        .map(|assignment| assignment.cell as usize)
        .collect::<std::collections::BTreeSet<_>>();
    let subduction_cells = plan
        .subduction_cells
        .iter()
        .map(|assignment| assignment.cell as usize)
        .collect::<std::collections::BTreeSet<_>>();
    let collision_cells = plan
        .primary_collision_cells
        .iter()
        .map(|&cell| cell as usize)
        .collect::<std::collections::BTreeSet<_>>();
    let transform_cells = plan
        .transform_cells
        .iter()
        .map(|&cell| cell as usize)
        .collect::<std::collections::BTreeSet<_>>();
    let mut diagnostics = SurfaceMarkerOwnershipDiagnostics::default();
    for (cell, (&candidate_count, (&before, &after))) in candidate_counts
        .iter()
        .zip(previous.iter().zip(&plate_id))
        .enumerate()
    {
        match candidate_count {
            0 => diagnostics.empty_candidate_cell_count += 1,
            1 => diagnostics.single_candidate_cell_count += 1,
            _ => diagnostics.mixed_candidate_cell_count += 1,
        }
        if before == after {
            continue;
        }
        match candidate_count {
            0 => diagnostics.changed_empty_candidate_cell_count += 1,
            1 => diagnostics.changed_single_candidate_cell_count += 1,
            _ => diagnostics.changed_mixed_candidate_cell_count += 1,
        }
        if previous_previous.get(cell).copied() == Some(after) {
            match candidate_count {
                0 => diagnostics.reversed_empty_candidate_cell_count += 1,
                1 => diagnostics.reversed_single_candidate_cell_count += 1,
                _ => diagnostics.reversed_mixed_candidate_cell_count += 1,
            }
        }
        diagnostics.changed_divergent_cell_count += u32::from(divergent_cells.contains(&cell));
        diagnostics.changed_subduction_cell_count += u32::from(subduction_cells.contains(&cell));
        diagnostics.changed_collision_cell_count += u32::from(collision_cells.contains(&cell));
        diagnostics.changed_transform_cell_count += u32::from(transform_cells.contains(&cell));
    }
    Ok((plate_id, diagnostics))
}

fn spherical_triangle_contains(vertices: [[f32; 3]; 3], point: [f32; 3]) -> bool {
    const CONTAINMENT_EPSILON: f32 = 1e-7;
    let signs = [
        dot(cross(vertices[0], vertices[1]), point),
        dot(cross(vertices[1], vertices[2]), point),
        dot(cross(vertices[2], vertices[0]), point),
    ];
    signs.iter().all(|&sign| sign >= -CONTAINMENT_EPSILON)
        || signs.iter().all(|&sign| sign <= CONTAINMENT_EPSILON)
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(super) fn reconstruct_surface_material_elements(
    projection: &SurfaceMaterialElementProjection,
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> Result<
    (
        Vec<SurfaceMaterialElementState>,
        MaterialElementReconstructionDiagnostics,
    ),
    String,
> {
    let dual_cells = build_barycentric_dual_cells(positions, nbr_offsets, nbrs)
        .ok_or_else(|| "failed to build dual cells for material reconstruction".to_string())?;
    if projection.cells.len() != dual_cells.len()
        || projection.target_cell_areas.len() != dual_cells.len()
    {
        return Err("material projection and target dual cells differ in length".to_string());
    }
    let mut elements = Vec::new();
    let mut diagnostics = MaterialElementReconstructionDiagnostics::default();
    for cell in 0..dual_cells.len() {
        let target_area = projection.target_cell_areas[cell];
        let representable_area = (target_area * MIN_REPRESENTABLE_CELL_FRACTION).max(AREA_EPSILON);
        let mut materials = projection.cells[cell]
            .iter()
            .copied()
            .filter(|material| material.area > representable_area)
            .collect::<Vec<_>>();
        let total_area = materials.iter().map(|material| material.area).sum::<f32>();
        if total_area <= AREA_EPSILON {
            return Err(format!("material reconstruction leaves cell {cell} empty"));
        }
        let closure = (total_area - target_area).abs();
        diagnostics.quadrature_closure_area += closure;
        diagnostics.max_relative_quadrature_closure = diagnostics
            .max_relative_quadrature_closure
            .max(closure / target_area.max(AREA_EPSILON));
        let scale = target_area / total_area;
        for material in &mut materials {
            scale_projected_material(material, scale);
        }
        if materials.len() > 1 {
            diagnostics.mixed_cell_count = diagnostics.mixed_cell_count.saturating_add(1);
        }
        materials.sort_by_key(|material| material.plate_id);
        let partition =
            reconstruct_multimaterial_mof(&dual_cells[cell], positions[cell], &materials)
                .ok_or_else(|| format!("failed to partition mixed material cell {cell}"))?;
        for (material, support) in partition {
            append_material_polygon_elements(
                &mut elements,
                cell,
                &material,
                &support,
                &mut diagnostics,
            )?;
        }
    }
    Ok((elements, diagnostics))
}

fn reconstruct_multimaterial_mof(
    polygon: &[[f32; 3]],
    center: [f32; 3],
    materials: &[ProjectedElementMaterial],
) -> Option<Vec<(ProjectedElementMaterial, Vec<[f32; 3]>)>> {
    if materials.is_empty() {
        return None;
    }
    let orders = material_orders(materials.len());
    let mut best = None::<(f32, Vec<(ProjectedElementMaterial, Vec<[f32; 3]>)>)>;
    for order in orders {
        let mut remaining_polygon = polygon.to_vec();
        let mut remaining_area = materials.iter().map(|material| material.area).sum::<f32>();
        let mut partition = Vec::with_capacity(materials.len());
        let mut defect = 0.0_f32;
        let mut valid = true;
        for (position, &material_index) in order.iter().enumerate() {
            let material = materials[material_index];
            let support = if position + 1 == order.len() {
                std::mem::take(&mut remaining_polygon)
            } else {
                let fraction = material.area / remaining_area.max(AREA_EPSILON);
                let Some((support, remainder, support_defect)) =
                    best_mof_cut(&remaining_polygon, center, &material, fraction)
                else {
                    valid = false;
                    break;
                };
                defect += support_defect;
                remaining_polygon = remainder;
                remaining_area -= material.area;
                support
            };
            if position + 1 == order.len() {
                defect += material_moment_defect(&material, &support);
            }
            partition.push((material, support));
        }
        if valid
            && best
                .as_ref()
                .is_none_or(|(best_defect, _)| defect < *best_defect)
        {
            best = Some((defect, partition));
        }
    }
    best.map(|(_, partition)| partition)
}

fn material_orders(count: usize) -> Vec<Vec<usize>> {
    let base = (0..count).collect::<Vec<_>>();
    if count > 4 {
        return vec![base];
    }
    fn visit(prefix: &mut Vec<usize>, remaining: &mut Vec<usize>, orders: &mut Vec<Vec<usize>>) {
        if remaining.is_empty() {
            orders.push(prefix.clone());
            return;
        }
        for index in 0..remaining.len() {
            let value = remaining.remove(index);
            prefix.push(value);
            visit(prefix, remaining, orders);
            prefix.pop();
            remaining.insert(index, value);
        }
    }
    let mut orders = Vec::new();
    visit(&mut Vec::new(), &mut base.clone(), &mut orders);
    orders
}

fn best_mof_cut(
    polygon: &[[f32; 3]],
    center: [f32; 3],
    material: &ProjectedElementMaterial,
    fraction: f32,
) -> Option<(Vec<[f32; 3]>, Vec<[f32; 3]>, f32)> {
    const DIRECTION_SAMPLES: usize = 16;
    const REFINEMENT_STEPS: usize = 8;

    let seed = if center[1].abs() < 0.9 {
        [0.0_f32, 1.0, 0.0]
    } else {
        [1.0_f32, 0.0, 0.0]
    };
    let tangent = normalized(cross(seed, center))?;
    let bitangent = normalized(cross(center, tangent))?;
    let mut best = None::<(f32, Vec<[f32; 3]>, Vec<[f32; 3]>, f32)>;
    for sample in 0..DIRECTION_SAMPLES {
        let angle = std::f32::consts::TAU * sample as f32 / DIRECTION_SAMPLES as f32;
        let Some((support, remainder, defect)) = mof_cut_at_angle(
            polygon, center, tangent, bitangent, angle, fraction, material,
        ) else {
            continue;
        };
        if best
            .as_ref()
            .is_none_or(|(_, _, _, best_defect)| defect < *best_defect)
        {
            best = Some((angle, support, remainder, defect));
        }
    }
    let (best_angle, _, _, _) = best?;
    let width = std::f32::consts::TAU / DIRECTION_SAMPLES as f32;
    let mut lower = best_angle - width;
    let mut upper = best_angle + width;
    for _ in 0..REFINEMENT_STEPS {
        let left = lower + (upper - lower) / 3.0;
        let right = upper - (upper - lower) / 3.0;
        let left_defect = mof_cut_at_angle(
            polygon, center, tangent, bitangent, left, fraction, material,
        )
        .map(|(_, _, defect)| defect)
        .unwrap_or(f32::INFINITY);
        let right_defect = mof_cut_at_angle(
            polygon, center, tangent, bitangent, right, fraction, material,
        )
        .map(|(_, _, defect)| defect)
        .unwrap_or(f32::INFINITY);
        if left_defect <= right_defect {
            upper = right;
        } else {
            lower = left;
        }
    }
    mof_cut_at_angle(
        polygon,
        center,
        tangent,
        bitangent,
        0.5 * (lower + upper),
        fraction,
        material,
    )
}

fn mof_cut_at_angle(
    polygon: &[[f32; 3]],
    center: [f32; 3],
    tangent: [f32; 3],
    bitangent: [f32; 3],
    angle: f32,
    fraction: f32,
    material: &ProjectedElementMaterial,
) -> Option<(Vec<[f32; 3]>, Vec<[f32; 3]>, f32)> {
    let direction = [
        tangent[0] * angle.cos() + bitangent[0] * angle.sin(),
        tangent[1] * angle.cos() + bitangent[1] * angle.sin(),
        tangent[2] * angle.cos() + bitangent[2] * angle.sin(),
    ];
    let toward = normalized([
        center[0] + 0.1 * direction[0],
        center[1] + 0.1 * direction[1],
        center[2] + 0.1 * direction[2],
    ])?;
    let (support, remainder) =
        cut_spherical_polygon_by_area_fraction(polygon, center, toward, fraction)?;
    let defect = material_moment_defect(material, &support);
    Some((support, remainder, defect))
}

fn material_moment_defect(material: &ProjectedElementMaterial, polygon: &[[f32; 3]]) -> f32 {
    let Some(actual) = spherical_polygon_first_moment(polygon) else {
        return f32::INFINITY;
    };
    (0..3)
        .map(|axis| {
            let difference = actual[axis] - material.first_moment[axis];
            difference * difference
        })
        .sum()
}

fn spherical_polygon_first_moment(polygon: &[[f32; 3]]) -> Option<[f32; 3]> {
    if polygon.len() < 3 {
        return None;
    }
    let mut moment = [0.0_f32; 3];
    for index in 1..polygon.len() - 1 {
        let vertices = [polygon[0], polygon[index], polygon[index + 1]];
        let area = spherical_triangle_area(vertices);
        let centroid = normalized([
            vertices[0][0] + vertices[1][0] + vertices[2][0],
            vertices[0][1] + vertices[1][1] + vertices[2][1],
            vertices[0][2] + vertices[1][2] + vertices[2][2],
        ])?;
        for axis in 0..3 {
            moment[axis] += centroid[axis] * area;
        }
    }
    Some(moment)
}

fn append_material_polygon_elements(
    elements: &mut Vec<SurfaceMaterialElementState>,
    host_cell: usize,
    material: &ProjectedElementMaterial,
    polygon: &[[f32; 3]],
    diagnostics: &mut MaterialElementReconstructionDiagnostics,
) -> Result<(), String> {
    let polygon = simplify_spherical_polygon(polygon);
    if polygon.len() < 3 {
        return Err(format!(
            "material partition produced a degenerate polygon: cell={host_cell}, plate={}, target_area={}, vertices={}",
            material.plate_id.0,
            material.area,
            polygon.len()
        ));
    }
    let center = normalized(polygon.iter().fold([0.0_f32; 3], |mut sum, point| {
        for axis in 0..3 {
            sum[axis] += point[axis];
        }
        sum
    }))
    .ok_or_else(|| "material partition produced an invalid center".to_string())?;
    let support_area = spherical_polygon_area(center, &polygon);
    if support_area <= AREA_EPSILON {
        return Err(format!(
            "material partition produced zero support area: cell={host_cell}, plate={}, target_area={}, vertices={}",
            material.plate_id.0,
            material.area,
            polygon.len()
        ));
    }
    diagnostics.reconstructed_area_error += (support_area - material.area).abs();
    let oceanic_fraction = material.oceanic_area / material.area.max(AREA_EPSILON);
    let mean_age = material.age_area / material.area.max(AREA_EPSILON);
    let mut reconstructed_moment = [0.0_f32; 3];
    for index in 1..polygon.len() - 1 {
        let vertices = [polygon[0], polygon[index], polygon[index + 1]];
        let area = spherical_triangle_area(vertices);
        if area <= AREA_EPSILON {
            continue;
        }
        let centroid = normalized([
            vertices[0][0] + vertices[1][0] + vertices[2][0],
            vertices[0][1] + vertices[1][1] + vertices[2][1],
            vertices[0][2] + vertices[1][2] + vertices[2][2],
        ])
        .ok_or_else(|| "material partition triangle has an invalid centroid".to_string())?;
        for axis in 0..3 {
            reconstructed_moment[axis] += centroid[axis] * area;
        }
        elements.push(SurfaceMaterialElementState {
            plate_id: material.plate_id,
            vertices,
            area,
            oceanic_area: area * oceanic_fraction,
            age_area: area * mean_age,
            host_cell: host_cell as u32,
            ownership_marker: true,
        });
    }
    diagnostics.first_moment_squared_error += (0..3)
        .map(|axis| {
            let difference = reconstructed_moment[axis] - material.first_moment[axis];
            difference * difference
        })
        .sum::<f32>();
    Ok(())
}

fn simplify_spherical_polygon(polygon: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let mut vertices = polygon.to_vec();
    loop {
        if vertices.len() <= 3 {
            break;
        }
        let center = normalized(vertices.iter().fold([0.0_f32; 3], |mut sum, point| {
            for axis in 0..3 {
                sum[axis] += point[axis];
            }
            sum
        }));
        let Some(center) = center else {
            break;
        };
        let area = spherical_polygon_area(center, &vertices);
        let removable = (0..vertices.len()).find(|&index| {
            let previous = vertices[(index + vertices.len() - 1) % vertices.len()];
            let current = vertices[index];
            let next = vertices[(index + 1) % vertices.len()];
            spherical_triangle_area([previous, current, next])
                <= area * MIN_REPRESENTABLE_CELL_FRACTION * 0.1
        });
        let Some(index) = removable else {
            break;
        };
        vertices.remove(index);
    }
    vertices
}

pub(super) fn surface_projection_from_elements(
    projection: &SurfaceMaterialElementProjection,
) -> SurfaceMaterialProjection {
    SurfaceMaterialProjection {
        cells: projection
            .cells
            .iter()
            .map(|materials| {
                materials
                    .iter()
                    .map(|material| ProjectedPlateMaterial {
                        plate_id: material.plate_id,
                        mass: material.area,
                        oceanic_mass: material.oceanic_area,
                        age_mass: material.age_area,
                    })
                    .collect()
            })
            .collect(),
        ..Default::default()
    }
}

pub(super) fn close_material_element_coverage(
    projection: &mut SurfaceMaterialElementProjection,
    plan: &SweptBoundaryPlan,
    positions: &[[f32; 3]],
) -> MaterialCoverageClosureDiagnostics {
    let mut diagnostics = MaterialCoverageClosureDiagnostics::default();
    for assignment in &plan.divergent_cells {
        let cell = assignment.cell as usize;
        let Some(materials) = projection.cells.get_mut(cell) else {
            continue;
        };
        let Some(&target_area) = projection.target_cell_areas.get(cell) else {
            continue;
        };
        let area = materials.iter().map(|material| material.area).sum::<f32>();
        let created_area = (target_area - area).max(0.0).min(assignment.mass);
        if created_area <= AREA_EPSILON {
            continue;
        }
        deposit_projected_element_material(
            materials,
            assignment.accreting_plate,
            created_area,
            created_area,
            0.0,
            positions[cell],
        );
        diagnostics.ridge_created_area += created_area;
    }
    for assignment in &plan.subduction_cells {
        let cell = assignment.cell as usize;
        let Some(materials) = projection.cells.get_mut(cell) else {
            continue;
        };
        let area = materials.iter().map(|material| material.area).sum::<f32>();
        let mut removal = (area - assignment.target_mass).max(0.0);
        let Some(material) = materials
            .iter_mut()
            .find(|material| material.plate_id == assignment.subducting_plate)
        else {
            continue;
        };
        removal = removal.min(material.oceanic_area).min(material.area);
        if removal <= AREA_EPSILON {
            continue;
        }
        scale_projected_material(material, (material.area - removal) / material.area);
        diagnostics.subducted_area += removal;
    }
    for &cell in &plan.primary_collision_cells {
        let cell = cell as usize;
        let Some(materials) = projection.cells.get_mut(cell) else {
            continue;
        };
        let Some(&target_area) = projection.target_cell_areas.get(cell) else {
            continue;
        };
        let area = materials.iter().map(|material| material.area).sum::<f32>();
        if area <= target_area + AREA_EPSILON {
            continue;
        }
        let scale = target_area / area;
        for material in materials {
            scale_projected_material(material, scale);
        }
        diagnostics.collision_excess_area += area - target_area;
    }
    for (materials, &target_area) in projection.cells.iter().zip(&projection.target_cell_areas) {
        let area = materials.iter().map(|material| material.area).sum::<f32>();
        diagnostics.residual_gap_area += (target_area - area).max(0.0);
        diagnostics.residual_overlap_area += (area - target_area).max(0.0);
    }
    diagnostics
}

fn deposit_projected_element_material(
    materials: &mut Vec<ProjectedElementMaterial>,
    plate_id: PlateId,
    area: f32,
    oceanic_area: f32,
    age_area: f32,
    centroid: [f32; 3],
) {
    let index = materials
        .iter()
        .position(|material| material.plate_id == plate_id)
        .unwrap_or_else(|| {
            materials.push(ProjectedElementMaterial {
                plate_id,
                ..Default::default()
            });
            materials.len() - 1
        });
    let material = &mut materials[index];
    material.area += area;
    material.oceanic_area += oceanic_area;
    material.age_area += age_area;
    for axis in 0..3 {
        material.first_moment[axis] += centroid[axis] * area;
    }
}

fn scale_projected_material(material: &mut ProjectedElementMaterial, scale: f32) {
    material.area *= scale;
    material.oceanic_area *= scale;
    material.age_area *= scale;
    for value in &mut material.first_moment {
        *value *= scale;
    }
}

pub(super) fn classify_material_coverage_regimes(
    projection: &SurfaceMaterialElementProjection,
    boundary_state: &BoundaryDynamicsState,
    plan: &SweptBoundaryPlan,
) -> MaterialCoverageRegimeDiagnostics {
    const RIDGE: u8 = 1 << 0;
    const SUBDUCTION: u8 = 1 << 1;
    const COLLISION: u8 = 1 << 2;

    let mut flags = vec![0_u8; projection.cells.len()];
    for (edge, boundary_type) in boundary_state
        .edge_pairs
        .iter()
        .zip(&boundary_state.edge_types)
    {
        let flag = match boundary_type {
            BoundaryType::Ridge | BoundaryType::Rift => RIDGE,
            BoundaryType::Subduction => SUBDUCTION,
            BoundaryType::Collision => COLLISION,
            BoundaryType::Transform | BoundaryType::PassiveMargin => 0,
        };
        for &cell in edge {
            if let Some(value) = flags.get_mut(cell as usize) {
                *value |= flag;
            }
        }
    }
    for assignment in &plan.divergent_cells {
        if let Some(value) = flags.get_mut(assignment.cell as usize) {
            *value |= RIDGE;
        }
    }
    for assignment in &plan.subduction_cells {
        if let Some(value) = flags.get_mut(assignment.cell as usize) {
            *value |= SUBDUCTION;
        }
    }
    for &cell in &plan.primary_collision_cells {
        if let Some(value) = flags.get_mut(cell as usize) {
            *value |= COLLISION;
        }
    }
    let mut diagnostics = MaterialCoverageRegimeDiagnostics::default();
    for (cell, (materials, &target_area)) in projection
        .cells
        .iter()
        .zip(&projection.target_cell_areas)
        .enumerate()
    {
        let area = materials.iter().map(|material| material.area).sum::<f32>();
        let gap = (target_area - area).max(0.0);
        let overlap = (area - target_area).max(0.0);
        if flags[cell] & RIDGE != 0 {
            diagnostics.ridge_gap_area += gap;
        } else {
            diagnostics.unsupported_gap_area += gap;
        }
        if flags[cell] & SUBDUCTION != 0 {
            diagnostics.subduction_overlap_area += overlap;
        } else if flags[cell] & COLLISION != 0 {
            diagnostics.collision_overlap_area += overlap;
        } else {
            diagnostics.unsupported_overlap_area += overlap;
        }
    }
    diagnostics
}

pub(super) fn initialize_surface_material_elements(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    crust: &[VertexCrustState],
) -> Result<Vec<SurfaceMaterialElementState>, String> {
    if positions.len() != plate_id.len() || positions.len() != crust.len() {
        return Err("surface material element inputs differ in length".to_string());
    }
    let dual_cells = build_barycentric_dual_cells(positions, nbr_offsets, nbrs)
        .ok_or_else(|| "failed to build dual cells for material elements".to_string())?;
    let mut elements = Vec::new();
    for cell in 0..positions.len() {
        let polygon = &dual_cells[cell];
        for index in 0..polygon.len() {
            let vertices = [
                positions[cell],
                polygon[index],
                polygon[(index + 1) % polygon.len()],
            ];
            let area = spherical_triangle_area(vertices);
            if area <= AREA_EPSILON {
                continue;
            }
            elements.push(SurfaceMaterialElementState {
                plate_id: plate_id[cell],
                vertices,
                area,
                oceanic_area: if crust[cell].crust_type == CrustType::Oceanic {
                    area
                } else {
                    0.0
                },
                age_area: crust[cell].age.max(0.0) * area,
                host_cell: cell as u32,
                ownership_marker: true,
            });
        }
    }
    Ok(elements)
}

pub(super) fn advect_surface_material_elements(
    elements: &mut [SurfaceMaterialElementState],
    plate_states: &[PlateKinematicsState],
) -> Result<(), String> {
    for element in elements {
        let state = plate_states
            .get(element.plate_id.as_usize())
            .ok_or_else(|| format!("plate {} has no kinematic state", element.plate_id.0))?;
        for vertex in &mut element.vertices {
            *vertex = rotate_unit_vector(*vertex, state.angular_axis, state.angular_speed)
                .ok_or_else(|| {
                    "material element rotation produced an invalid vertex".to_string()
                })?;
        }
    }
    Ok(())
}

pub(super) fn project_surface_material_elements(
    elements: &mut [SurfaceMaterialElementState],
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> Result<SurfaceMaterialElementProjection, String> {
    let dual_cells = build_barycentric_dual_cells(positions, nbr_offsets, nbrs)
        .ok_or_else(|| "failed to build target dual cells for material elements".to_string())?;
    let target_cell_areas = dual_cells
        .iter()
        .enumerate()
        .map(|(cell, polygon)| spherical_polygon_area(positions[cell], polygon))
        .collect::<Vec<_>>();
    let mut projection = SurfaceMaterialElementProjection {
        cells: vec![Vec::new(); positions.len()],
        target_cell_areas,
        ..Default::default()
    };
    for element in elements {
        projection.input_area += element.area;
        let center = normalized([
            element.vertices[0][0] + element.vertices[1][0] + element.vertices[2][0],
            element.vertices[0][1] + element.vertices[1][1] + element.vertices[2][1],
            element.vertices[0][2] + element.vertices[1][2] + element.vertices[2][2],
        ])
        .ok_or_else(|| "material element has an invalid center".to_string())?;
        let host = nearest_mesh_cell(
            center,
            element.host_cell as usize,
            positions,
            nbr_offsets,
            nbrs,
        )
        .ok_or_else(|| "material element has no target host cell".to_string())?;
        element.host_cell = host as u32;
        let Some(overlap) = polygon_overlap_fractions(
            &element.vertices,
            center,
            host,
            nbr_offsets,
            nbrs,
            &dual_cells,
        ) else {
            projection.unassigned_element_count =
                projection.unassigned_element_count.saturating_add(1);
            projection.unassigned_element_area += element.area;
            projection.max_unassigned_element_area =
                projection.max_unassigned_element_area.max(element.area);
            continue;
        };
        for overlap in overlap.fractions {
            let target = overlap.target;
            let fraction = overlap.fraction;
            let area = element.area * fraction;
            deposit_element_material(
                &mut projection.cells[target],
                element,
                area,
                fraction,
                overlap.centroid,
            );
            projection.projected_area += area;
        }
    }
    for (materials, &target_area) in projection.cells.iter().zip(&projection.target_cell_areas) {
        let projected_area = materials.iter().map(|material| material.area).sum::<f32>();
        projection.uncovered_area += (target_area - projected_area).max(0.0);
        projection.overlap_area += (projected_area - target_area).max(0.0);
        projection.absolute_coverage_error += (projected_area - target_area).abs();
    }
    Ok(projection)
}

fn deposit_element_material(
    materials: &mut Vec<ProjectedElementMaterial>,
    element: &SurfaceMaterialElementState,
    area: f32,
    fraction: f32,
    centroid: [f32; 3],
) {
    let index = materials
        .iter()
        .position(|material| material.plate_id == element.plate_id)
        .unwrap_or_else(|| {
            materials.push(ProjectedElementMaterial {
                plate_id: element.plate_id,
                ..Default::default()
            });
            materials.len() - 1
        });
    let material = &mut materials[index];
    material.area += area;
    material.oceanic_area += element.oceanic_area * fraction;
    material.age_area += element.age_area * fraction;
    for axis in 0..3 {
        material.first_moment[axis] += centroid[axis] * area;
    }
}

fn spherical_polygon_area(center: [f32; 3], polygon: &[[f32; 3]]) -> f32 {
    (0..polygon.len())
        .map(|index| {
            spherical_triangle_area([center, polygon[index], polygon[(index + 1) % polygon.len()]])
        })
        .sum()
}

fn spherical_triangle_area(vertices: [[f32; 3]; 3]) -> f32 {
    let [a, b, c] = vertices;
    let cross = [
        b[1] * c[2] - b[2] * c[1],
        b[2] * c[0] - b[0] * c[2],
        b[0] * c[1] - b[1] * c[0],
    ];
    let numerator = dot(a, cross).abs();
    let denominator = 1.0 + dot(a, b) + dot(b, c) + dot(c, a);
    2.0 * numerator.atan2(denominator.max(AREA_EPSILON))
}

fn normalized(value: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot(value, value).sqrt();
    if !length.is_finite() || length <= AREA_EPSILON {
        return None;
    }
    Some([value[0] / length, value[1] / length, value[2] / length])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::mesh::{build_neighbors, generate_icosphere};
    use crate::GeologyParams;

    fn plate_state(axis: [f32; 3], speed: f32) -> PlateKinematicsState {
        PlateKinematicsState {
            angular_axis: normalized(axis).unwrap(),
            angular_speed: speed,
            reference_angular_speed: speed,
            slab_pull_drive: 0.0,
            ridge_push_drive: 0.0,
            collision_drag: 0.0,
            force_target_speed_km_per_myr: 0.0,
            basal_target_speed_km_per_myr: 0.0,
            phase_offset: 0.0,
            activity: 1.0,
        }
    }

    fn crust() -> VertexCrustState {
        VertexCrustState {
            crust_type: CrustType::Oceanic,
            thickness: 1.0,
            density: 3_000.0,
            age: 30.0,
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

    fn setup(
        level: u32,
    ) -> (
        Vec<[f32; 3]>,
        Vec<u32>,
        Vec<u32>,
        Vec<SurfaceMaterialElementState>,
    ) {
        let (positions, indices) = generate_icosphere(level);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let plate_id = vec![PlateId(0); positions.len()];
        let elements = initialize_surface_material_elements(
            &positions,
            &nbr_offsets,
            &nbrs,
            &plate_id,
            &vec![crust(); positions.len()],
        )
        .unwrap();
        (positions, nbr_offsets, nbrs, elements)
    }

    #[test]
    fn identity_projection_preserves_dual_cell_coverage() {
        let (positions, nbr_offsets, nbrs, mut elements) = setup(2);

        let projection =
            project_surface_material_elements(&mut elements, &positions, &nbr_offsets, &nbrs)
                .unwrap();

        assert_eq!(projection.unassigned_element_count, 0);
        assert!((projection.input_area - 4.0 * std::f32::consts::PI).abs() < 1e-4);
        assert!(projection.uncovered_area < 1e-5);
        assert!(projection.overlap_area < 1e-5);
    }

    #[test]
    fn mof_search_recovers_known_oblique_interface_moment() {
        let (positions, nbr_offsets, nbrs, _) = setup(2);
        let dual_cells = build_barycentric_dual_cells(&positions, &nbr_offsets, &nbrs).unwrap();
        let center = positions[0];
        let polygon = &dual_cells[0];
        let toward = normalized([center[0] + 0.07, center[1] - 0.11, center[2] + 0.03]).unwrap();
        let fraction = 0.37;
        let (expected_support, _) =
            cut_spherical_polygon_by_area_fraction(polygon, center, toward, fraction).unwrap();
        let area = spherical_polygon_area_vertices(&expected_support).unwrap();
        let material = ProjectedElementMaterial {
            plate_id: PlateId(0),
            area,
            first_moment: spherical_polygon_first_moment(&expected_support).unwrap(),
            ..Default::default()
        };

        let (actual_support, _, defect) =
            best_mof_cut(polygon, center, &material, fraction).unwrap();
        let actual_area = spherical_polygon_area_vertices(&actual_support).unwrap();

        assert!(
            (actual_area - area).abs() < 1e-6,
            "expected_area={area}, actual_area={actual_area}"
        );
        assert!(defect < 1e-8, "first-moment defect={defect}");
    }

    #[test]
    fn multimaterial_mof_is_input_order_invariant() {
        let (positions, nbr_offsets, nbrs, _) = setup(2);
        let dual_cells = build_barycentric_dual_cells(&positions, &nbr_offsets, &nbrs).unwrap();
        let center = positions[0];
        let polygon = &dual_cells[0];
        let toward_a = positions[nbrs[nbr_offsets[0] as usize] as usize];
        let toward_b = positions[nbrs[nbr_offsets[0] as usize + 2] as usize];
        let (support_a, remainder) =
            cut_spherical_polygon_by_area_fraction(polygon, center, toward_a, 0.22).unwrap();
        let (support_b, support_c) =
            cut_spherical_polygon_by_area_fraction(&remainder, center, toward_b, 0.46).unwrap();
        let material = |plate_id, support: &Vec<[f32; 3]>| ProjectedElementMaterial {
            plate_id: PlateId(plate_id),
            area: spherical_polygon_area_vertices(support).unwrap(),
            first_moment: spherical_polygon_first_moment(support).unwrap(),
            ..Default::default()
        };
        let ordered = vec![
            material(0, &support_a),
            material(1, &support_b),
            material(2, &support_c),
        ];
        let shuffled = vec![ordered[2], ordered[0], ordered[1]];

        let first = reconstruct_multimaterial_mof(polygon, center, &ordered).unwrap();
        let second = reconstruct_multimaterial_mof(polygon, center, &shuffled).unwrap();
        let defects = |partition: &[(ProjectedElementMaterial, Vec<[f32; 3]>)]| {
            partition
                .iter()
                .map(|(material, support)| material_moment_defect(material, support))
                .sum::<f32>()
        };

        assert!(defects(&first) < 1e-7, "defect={}", defects(&first));
        assert!((defects(&first) - defects(&second)).abs() < 1e-9);
    }

    #[test]
    fn common_rigid_rotation_preserves_full_sphere_coverage() {
        let (positions, nbr_offsets, nbrs, mut elements) = setup(3);
        advect_surface_material_elements(&mut elements, &[plate_state([0.3, 0.7, -0.2], 0.08)])
            .unwrap();

        let projection =
            project_surface_material_elements(&mut elements, &positions, &nbr_offsets, &nbrs)
                .unwrap();

        assert_eq!(projection.unassigned_element_count, 0);
        assert!(
            (projection.projected_area - projection.input_area).abs() < 2e-4,
            "input={} projected={} unassigned={}",
            projection.input_area,
            projection.projected_area,
            projection.unassigned_element_count
        );
        assert!(
            projection.uncovered_area < 2e-3,
            "uncovered={}",
            projection.uncovered_area
        );
        assert!(
            projection.overlap_area < 2e-3,
            "overlap={}",
            projection.overlap_area
        );
        let (mut reconstructed, diagnostics) =
            reconstruct_surface_material_elements(&projection, &positions, &nbr_offsets, &nbrs)
                .unwrap();
        let reconstructed_projection =
            project_surface_material_elements(&mut reconstructed, &positions, &nbr_offsets, &nbrs)
                .unwrap();
        assert!(diagnostics.reconstructed_area_error < 2e-4);
        assert!(reconstructed_projection.uncovered_area < 1e-5);
        assert!(reconstructed_projection.overlap_area < 1e-5);
    }

    #[test]
    fn differential_plate_rotation_confines_coverage_defects_to_boundary_band() {
        let params = GeologyParams {
            level: 4,
            ..GeologyParams::default()
        };
        let (geology, positions, nbr_offsets, nbrs) =
            crate::sim::build_geology_with_mesh("alpha", params);
        let crust = vec![crust(); positions.len()];
        let mut elements = initialize_surface_material_elements(
            &positions,
            &nbr_offsets,
            &nbrs,
            &geology.plate_id,
            &crust,
        )
        .unwrap();
        let plate_states = geology
            .initial_plate_kinematics
            .iter()
            .map(|state| plate_state(state.angular_axis, state.angular_speed))
            .collect::<Vec<_>>();
        let mut boundary_state = BoundaryDynamicsState::default();
        super::super::boundary_dynamics::reclassify_boundaries(
            super::super::boundary_dynamics::ReclassifyBoundariesInput {
                positions: &positions,
                nbr_offsets: &nbr_offsets,
                nbrs: &nbrs,
                plate_id: &geology.plate_id,
                plate_states: &plate_states,
                vertex_states: &crust,
                params: &GeologyParams {
                    level: 4,
                    ..GeologyParams::default()
                },
            },
            &mut boundary_state,
        );

        advect_surface_material_elements(&mut elements, &plate_states).unwrap();
        let mut projection =
            project_surface_material_elements(&mut elements, &positions, &nbr_offsets, &nbrs)
                .unwrap();
        let boundary_distance =
            distance_from_plate_boundary(&geology.plate_id, &nbr_offsets, &nbrs);
        let defect_cells = projection
            .cells
            .iter()
            .zip(&projection.target_cell_areas)
            .enumerate()
            .filter_map(|(cell, (materials, &target_area))| {
                let area = materials.iter().map(|material| material.area).sum::<f32>();
                ((area - target_area).abs() > target_area * 0.02).then_some(cell)
            })
            .collect::<Vec<_>>();
        let regimes = classify_material_coverage_regimes(
            &projection,
            &boundary_state,
            &SweptBoundaryPlan::default(),
        );
        let bridge_projection = surface_projection_from_elements(&projection);
        let plan = super::super::surface_boundary_sweep::plan_swept_boundary_reactions(
            super::super::surface_boundary_sweep::SweptBoundaryInput {
                positions: &positions,
                nbr_offsets: &nbr_offsets,
                nbrs: &nbrs,
                plate_id: &geology.plate_id,
                crust: &crust,
                plate_states: &plate_states,
                boundary_state: &boundary_state,
                projection: &bridge_projection,
                cell_capacity: Some(&projection.target_cell_areas),
            },
        );
        let closure = close_material_element_coverage(&mut projection, &plan, &positions);

        assert_eq!(projection.unassigned_element_count, 0);
        assert!(projection.uncovered_area > 0.0);
        assert!(projection.overlap_area > 0.0);
        assert!(
            (projection.projected_area - projection.input_area).abs() < 2e-4,
            "input={} projected={}",
            projection.input_area,
            projection.projected_area
        );
        assert!(!defect_cells.is_empty());
        assert!(
            defect_cells
                .iter()
                .all(|&cell| boundary_distance[cell] <= 4),
            "coverage defect escaped the four-cell boundary band"
        );
        let supported_gap_ratio = regimes.ridge_gap_area
            / (regimes.ridge_gap_area + regimes.unsupported_gap_area).max(AREA_EPSILON);
        let supported_overlap_ratio = (regimes.subduction_overlap_area
            + regimes.collision_overlap_area)
            / (regimes.subduction_overlap_area
                + regimes.collision_overlap_area
                + regimes.unsupported_overlap_area)
                .max(AREA_EPSILON);
        assert!(
            supported_gap_ratio > 0.5,
            "gap support={supported_gap_ratio:?}, regimes={regimes:?}"
        );
        assert!(
            supported_overlap_ratio > 0.5,
            "overlap support={supported_overlap_ratio:?}, regimes={regimes:?}"
        );
        assert!(closure.ridge_created_area > 0.0, "closure={closure:?}");
        assert!(
            closure.subducted_area + closure.collision_excess_area > 0.0,
            "closure={closure:?}"
        );
        assert!(
            closure.residual_gap_area < projection.uncovered_area * 0.01,
            "closure={closure:?}"
        );
        assert!(
            closure.residual_overlap_area < projection.overlap_area * 0.01,
            "closure={closure:?}"
        );
        let (mut reconstructed, reconstruction) =
            reconstruct_surface_material_elements(&projection, &positions, &nbr_offsets, &nbrs)
                .unwrap();
        let reconstructed_projection =
            project_surface_material_elements(&mut reconstructed, &positions, &nbr_offsets, &nbrs)
                .unwrap();
        assert!(
            reconstruction.reconstructed_area_error < 2e-4,
            "reconstruction={reconstruction:?}"
        );
        assert!(reconstructed_projection.uncovered_area < 1e-5);
        assert!(reconstructed_projection.overlap_area < 1e-5);
    }

    fn distance_from_plate_boundary(
        plate_id: &[PlateId],
        nbr_offsets: &[u32],
        nbrs: &[u32],
    ) -> Vec<u32> {
        let mut distance = vec![u32::MAX; plate_id.len()];
        let mut queue = std::collections::VecDeque::new();
        for cell in 0..plate_id.len() {
            let start = nbr_offsets[cell] as usize;
            let end = nbr_offsets[cell + 1] as usize;
            if nbrs[start..end]
                .iter()
                .any(|&neighbor| plate_id[neighbor as usize] != plate_id[cell])
            {
                distance[cell] = 0;
                queue.push_back(cell);
            }
        }
        while let Some(cell) = queue.pop_front() {
            let start = nbr_offsets[cell] as usize;
            let end = nbr_offsets[cell + 1] as usize;
            for &neighbor in &nbrs[start..end] {
                let neighbor = neighbor as usize;
                if distance[neighbor] == u32::MAX {
                    distance[neighbor] = distance[cell].saturating_add(1);
                    queue.push_back(neighbor);
                }
            }
        }
        distance
    }
}
