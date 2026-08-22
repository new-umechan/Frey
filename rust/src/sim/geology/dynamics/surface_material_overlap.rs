use crate::sim::exec::math::{cross3, dot};
use crate::sim::geology_types::PlateId;
use crate::sim::world::{PlateKinematicsState, SurfaceMaterialState, VertexCrustState};
use smallvec::SmallVec;

use super::surface_cell_geometry::build_barycentric_dual_cells;
use super::surface_material_projection::{
    deposit_projected_mass_components, finish_projection_diagnostics, SurfaceMaterialProjection,
};
use super::surface_material_transport::{nearest_mesh_cell, rotate_unit_vector};

const AREA_EPSILON: f32 = 1e-10;
const PROJECTION_EPSILON: f32 = 1e-6;
const OVERLAP_RELATIVE_EPSILON: f32 = 1e-6;
type Polygon2 = SmallVec<[Point2; 12]>;

pub(super) struct DualCellRemapInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub nbr_offsets: &'a [u32],
    pub nbrs: &'a [u32],
    pub plate_id: &'a [PlateId],
    pub crust: &'a [VertexCrustState],
    pub plate_states: &'a [PlateKinematicsState],
    pub source_material: Option<&'a [Vec<SurfaceMaterialState>]>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct DualCellRemapDiagnostics {
    pub source_cell_count: u32,
    pub deposited_source_cell_count: u32,
    pub unassigned_source_cell_count: u32,
    pub invalid_source_cell_count: u32,
    pub tested_candidate_count: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct DualCellRemap {
    pub projection: SurfaceMaterialProjection,
    pub diagnostics: DualCellRemapDiagnostics,
}

pub(super) fn remap_dual_cell_material(input: DualCellRemapInput<'_>) -> DualCellRemap {
    let cell_count = input.positions.len();
    let mut remap = DualCellRemap {
        projection: SurfaceMaterialProjection {
            cells: vec![Vec::new(); cell_count],
            ..Default::default()
        },
        ..Default::default()
    };
    remap.diagnostics.source_cell_count = cell_count as u32;
    remap.projection.diagnostics.input_mass = input
        .source_material
        .map(|cells| {
            cells
                .iter()
                .flat_map(|materials| materials.iter())
                .map(|material| material.mass.max(0.0))
                .sum()
        })
        .unwrap_or(cell_count as f32);
    let Some(dual_cells) =
        build_barycentric_dual_cells(input.positions, input.nbr_offsets, input.nbrs)
    else {
        remap.diagnostics.invalid_source_cell_count = cell_count as u32;
        finish_projection_diagnostics(&mut remap.projection);
        return remap;
    };

    for source in 0..cell_count {
        remap_source_cell(source, &input, &dual_cells, &mut remap);
    }
    finish_projection_diagnostics(&mut remap.projection);
    remap
}

fn remap_source_cell(
    source: usize,
    input: &DualCellRemapInput<'_>,
    dual_cells: &[Vec<[f32; 3]>],
    remap: &mut DualCellRemap,
) {
    let Some(source_polygon) = dual_cells.get(source) else {
        record_invalid_source(remap);
        return;
    };
    let source_materials = if let Some(cells) = input.source_material {
        let Some(materials) = cells.get(source) else {
            record_invalid_source(remap);
            return;
        };
        materials.clone()
    } else {
        let (Some(&plate_id), Some(crust)) = (input.plate_id.get(source), input.crust.get(source))
        else {
            record_invalid_source(remap);
            return;
        };
        vec![SurfaceMaterialState {
            plate_id,
            mass: 1.0,
            oceanic_mass: if crust.crust_type == crate::sim::geology_types::CrustType::Oceanic {
                1.0
            } else {
                0.0
            },
            age_mass: crust.age,
        }]
    };
    if source_materials.is_empty() {
        remap.diagnostics.deposited_source_cell_count = remap
            .diagnostics
            .deposited_source_cell_count
            .saturating_add(1);
        return;
    }
    let mut failed = None;
    let mut active_material_count = 0_u32;
    for material in source_materials {
        if material.mass <= AREA_EPSILON {
            continue;
        }
        active_material_count = active_material_count.saturating_add(1);
        if let Err(error) =
            remap_source_material(source, source_polygon, material, input, dual_cells, remap)
        {
            failed = Some(error);
            break;
        }
    }
    if active_material_count == 0 {
        remap.diagnostics.deposited_source_cell_count = remap
            .diagnostics
            .deposited_source_cell_count
            .saturating_add(1);
        return;
    }
    match failed {
        None => {
            remap.diagnostics.deposited_source_cell_count = remap
                .diagnostics
                .deposited_source_cell_count
                .saturating_add(1);
        }
        Some(RemapMaterialError::Unassigned) => {
            remap.diagnostics.unassigned_source_cell_count = remap
                .diagnostics
                .unassigned_source_cell_count
                .saturating_add(1);
        }
        Some(RemapMaterialError::Invalid) => record_invalid_source(remap),
    }
}

#[derive(Clone, Copy)]
enum RemapMaterialError {
    Unassigned,
    Invalid,
}

fn remap_source_material(
    source: usize,
    source_polygon: &[[f32; 3]],
    material: SurfaceMaterialState,
    input: &DualCellRemapInput<'_>,
    dual_cells: &[Vec<[f32; 3]>],
    remap: &mut DualCellRemap,
) -> Result<(), RemapMaterialError> {
    let state = input
        .plate_states
        .get(material.plate_id.as_usize())
        .ok_or(RemapMaterialError::Invalid)?;
    let rotated_center = rotate_unit_vector(
        input.positions[source],
        state.angular_axis,
        state.angular_speed,
    )
    .ok_or(RemapMaterialError::Invalid)?;
    let rotated_polygon =
        rotate_polygon(source_polygon, state).ok_or(RemapMaterialError::Invalid)?;
    let host = nearest_mesh_cell(
        rotated_center,
        source,
        input.positions,
        input.nbr_offsets,
        input.nbrs,
    )
    .ok_or(RemapMaterialError::Invalid)?;
    let overlap = polygon_overlap_fractions(
        &rotated_polygon,
        rotated_center,
        host,
        input.nbr_offsets,
        input.nbrs,
        dual_cells,
    )
    .ok_or(RemapMaterialError::Unassigned)?;
    remap.diagnostics.tested_candidate_count = remap
        .diagnostics
        .tested_candidate_count
        .saturating_add(overlap.tested_candidate_count);
    for overlap in overlap.fractions {
        let target = overlap.target;
        let fraction = overlap.fraction;
        deposit_projected_mass_components(
            &mut remap.projection.cells[target],
            material.plate_id,
            material.mass * fraction,
            material.oceanic_mass * fraction,
            material.age_mass * fraction,
        );
        remap.projection.diagnostics.projected_mass += material.mass * fraction;
    }
    Ok(())
}

pub(super) struct PolygonOverlapFractions {
    pub fractions: SmallVec<[PolygonOverlapFraction; 7]>,
    pub tested_candidate_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PolygonOverlapFraction {
    pub target: usize,
    pub fraction: f32,
    pub centroid: [f32; 3],
}

pub(super) fn polygon_overlap_fractions(
    polygon: &[[f32; 3]],
    center: [f32; 3],
    host: usize,
    nbr_offsets: &[u32],
    nbrs: &[u32],
    dual_cells: &[Vec<[f32; 3]>],
) -> Option<PolygonOverlapFractions> {
    let frame = GnomonicFrame::new(center)?;
    let subject = frame.project_polygon(polygon)?;
    let subject_area = signed_area(&subject).abs();
    if !subject_area.is_finite() || subject_area <= 0.0 {
        return None;
    }
    let mut tested_candidate_count = 0_u32;
    let mut overlaps = SmallVec::<[(usize, f32, [f32; 3]); 7]>::new();
    for target in target_candidates(host, nbr_offsets, nbrs) {
        tested_candidate_count = tested_candidate_count.saturating_add(1);
        let target_polygon = dual_cells.get(target)?;
        let clip_polygon = frame.project_polygon(target_polygon)?;
        let clipped = clip_polygon_intersection(&subject, &clip_polygon);
        let area = signed_area(&clipped).abs();
        // This is a projection partition, not a material-pruning step. The
        // fragment lifetime is controlled by `discard_subcell_material_dust`.
        // Filter only numerical slivers relative to this subject polygon;
        // a global area cutoff incorrectly discards an entire thin fragment.
        if area.is_finite() && area > subject_area * OVERLAP_RELATIVE_EPSILON {
            overlaps.push((target, area, frame.polygon_centroid(&clipped)?));
        }
    }
    let total_area = overlaps.iter().map(|(_, area, _)| *area).sum::<f32>();
    if !total_area.is_finite() || total_area <= 0.0 {
        return None;
    }
    Some(PolygonOverlapFractions {
        fractions: overlaps
            .into_iter()
            .map(|(target, area, centroid)| PolygonOverlapFraction {
                target,
                fraction: area / total_area,
                centroid,
            })
            .collect(),
        tested_candidate_count,
    })
}

pub(super) fn polygon_overlap_failure_details(
    polygon: &[[f32; 3]],
    center: [f32; 3],
    host: usize,
    nbr_offsets: &[u32],
    nbrs: &[u32],
    dual_cells: &[Vec<[f32; 3]>],
) -> String {
    let Some(frame) = GnomonicFrame::new(center) else {
        return "stage=frame".to_string();
    };
    let Some(subject) = frame.project_polygon(polygon) else {
        return format!(
            "stage=subject_projection vertex_dot_min={:.9}",
            polygon
                .iter()
                .map(|point| dot(*point, center))
                .fold(f32::INFINITY, f32::min)
        );
    };
    let mut projected_candidates = 0usize;
    let mut nonempty_clips = 0usize;
    let mut max_clip_area = 0.0_f32;
    for target in target_candidates(host, nbr_offsets, nbrs) {
        let Some(target_polygon) = dual_cells.get(target) else {
            continue;
        };
        let Some(clip_polygon) = frame.project_polygon(target_polygon) else {
            continue;
        };
        projected_candidates += 1;
        let clipped = clip_polygon_intersection(&subject, &clip_polygon);
        if clipped.len() >= 3 {
            nonempty_clips += 1;
        }
        max_clip_area = max_clip_area.max(signed_area(&clipped).abs());
    }
    let max_edge_angle = polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
        .map(|(&a, &b)| dot(a, b).clamp(-1.0, 1.0).acos())
        .fold(0.0_f32, f32::max);
    format!(
        "stage=clip host={host} subject_area={:.12} projected_candidates={projected_candidates} \
         nonempty_clips={nonempty_clips} max_clip_area={max_clip_area:.12} max_edge_angle={max_edge_angle:.9}",
        signed_area(&subject).abs(),
    )
}

pub(super) fn cut_spherical_polygon_by_area_fraction(
    polygon: &[[f32; 3]],
    center: [f32; 3],
    toward: [f32; 3],
    retained_fraction: f32,
) -> Option<(Vec<[f32; 3]>, Vec<[f32; 3]>)> {
    let frame = GnomonicFrame::new(center)?;
    let polygon_2d = frame.project_polygon(polygon)?;
    let direction = frame.project(toward)?;
    let length = (direction.x * direction.x + direction.y * direction.y).sqrt();
    if length <= PROJECTION_EPSILON {
        return None;
    }
    let normal = Point2 {
        x: direction.x / length,
        y: direction.y / length,
    };
    let mut lower = polygon_2d
        .iter()
        .map(|point| dot2(*point, normal))
        .fold(f32::INFINITY, f32::min);
    let mut upper = polygon_2d
        .iter()
        .map(|point| dot2(*point, normal))
        .fold(f32::NEG_INFINITY, f32::max);
    let target_area = spherical_polygon_area_vertices(polygon)? * retained_fraction.clamp(0.0, 1.0);
    for _ in 0..32 {
        let threshold = 0.5 * (lower + upper);
        let retained = clip_half_plane(&polygon_2d, normal, threshold, true);
        let retained_3d = retained
            .into_iter()
            .map(|point| frame.unproject(point))
            .collect::<Option<Vec<_>>>()?;
        if spherical_polygon_area_vertices(&retained_3d)? > target_area {
            lower = threshold;
        } else {
            upper = threshold;
        }
    }
    let threshold = 0.5 * (lower + upper);
    let retained = clip_half_plane(&polygon_2d, normal, threshold, true);
    let remainder = clip_half_plane(&polygon_2d, normal, threshold, false);
    Some((
        retained
            .into_iter()
            .map(|point| frame.unproject(point))
            .collect::<Option<Vec<_>>>()?,
        remainder
            .into_iter()
            .map(|point| frame.unproject(point))
            .collect::<Option<Vec<_>>>()?,
    ))
}

pub(super) fn spherical_polygon_area_vertices(polygon: &[[f32; 3]]) -> Option<f32> {
    if polygon.len() < 3 {
        return Some(0.0);
    }
    let center = normalized(polygon.iter().fold([0.0_f32; 3], |mut sum, point| {
        for axis in 0..3 {
            sum[axis] += point[axis];
        }
        sum
    }))?;
    Some(
        polygon
            .iter()
            .zip(polygon.iter().cycle().skip(1))
            .take(polygon.len())
            .map(|(&a, &b)| spherical_triangle_area(center, a, b))
            .sum(),
    )
}

fn spherical_triangle_area(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    let cross = cross3(b, c);
    let numerator = dot(a, cross).abs();
    let denominator = 1.0 + dot(a, b) + dot(b, c) + dot(c, a);
    2.0 * numerator.atan2(denominator.max(AREA_EPSILON))
}

fn record_invalid_source(remap: &mut DualCellRemap) {
    remap.diagnostics.invalid_source_cell_count = remap
        .diagnostics
        .invalid_source_cell_count
        .saturating_add(1);
}

fn rotate_polygon(polygon: &[[f32; 3]], state: &PlateKinematicsState) -> Option<Vec<[f32; 3]>> {
    polygon
        .iter()
        .map(|&point| rotate_unit_vector(point, state.angular_axis, state.angular_speed))
        .collect()
}

fn target_candidates(host: usize, nbr_offsets: &[u32], nbrs: &[u32]) -> SmallVec<[usize; 7]> {
    let mut candidates = SmallVec::new();
    candidates.push(host);
    if let Some(neighbors) = cell_neighbors(host, nbr_offsets, nbrs) {
        candidates.extend(neighbors.iter().map(|&cell| cell as usize));
    }
    candidates
}

#[derive(Clone, Copy)]
struct Point2 {
    x: f32,
    y: f32,
}

struct GnomonicFrame {
    center: [f32; 3],
    tangent: [f32; 3],
    bitangent: [f32; 3],
}

impl GnomonicFrame {
    fn new(center: [f32; 3]) -> Option<Self> {
        let seed = if center[1].abs() < 0.95 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let tangent = normalized(cross3(seed, center))?;
        let bitangent = normalized(cross3(center, tangent))?;
        Some(Self {
            center,
            tangent,
            bitangent,
        })
    }

    fn project_polygon(&self, polygon: &[[f32; 3]]) -> Option<Polygon2> {
        let mut projected = polygon
            .iter()
            .map(|&point| self.project(point))
            .collect::<Option<Polygon2>>()?;
        if signed_area(&projected) < 0.0 {
            projected.reverse();
        }
        Some(projected)
    }

    fn project(&self, point: [f32; 3]) -> Option<Point2> {
        let denominator = dot(point, self.center);
        if !denominator.is_finite() || denominator <= PROJECTION_EPSILON {
            return None;
        }
        Some(Point2 {
            x: dot(point, self.tangent) / denominator,
            y: dot(point, self.bitangent) / denominator,
        })
    }

    fn unproject(&self, point: Point2) -> Option<[f32; 3]> {
        normalized([
            self.center[0] + self.tangent[0] * point.x + self.bitangent[0] * point.y,
            self.center[1] + self.tangent[1] * point.x + self.bitangent[1] * point.y,
            self.center[2] + self.tangent[2] * point.x + self.bitangent[2] * point.y,
        ])
    }

    fn polygon_centroid(&self, polygon: &[Point2]) -> Option<[f32; 3]> {
        let centroid = polygon_centroid(polygon)?;
        self.unproject(centroid)
    }
}

#[cfg(test)]
fn clipped_polygon_area(subject: &[Point2], clip_polygon: &[Point2]) -> f32 {
    signed_area(&clip_polygon_intersection(subject, clip_polygon)).abs()
}

fn clip_polygon_intersection(subject: &[Point2], clip_polygon: &[Point2]) -> Polygon2 {
    let mut output = subject.iter().copied().collect::<Polygon2>();
    for index in 0..clip_polygon.len() {
        let clip_start = clip_polygon[index];
        let clip_end = clip_polygon[(index + 1) % clip_polygon.len()];
        output = clip_against_edge(&output, clip_start, clip_end);
        if output.len() < 3 {
            return Polygon2::new();
        }
    }
    output
}

fn clip_against_edge(subject: &[Point2], clip_start: Point2, clip_end: Point2) -> Polygon2 {
    let mut output = Polygon2::new();
    let Some(mut previous) = subject.last().copied() else {
        return output;
    };
    let mut previous_inside = is_inside(previous, clip_start, clip_end);
    for &current in subject {
        let current_inside = is_inside(current, clip_start, clip_end);
        if current_inside != previous_inside {
            if let Some(intersection) = line_intersection(previous, current, clip_start, clip_end) {
                output.push(intersection);
            }
        }
        if current_inside {
            output.push(current);
        }
        previous = current;
        previous_inside = current_inside;
    }
    output
}

fn clip_half_plane(
    polygon: &[Point2],
    normal: Point2,
    threshold: f32,
    retain_above: bool,
) -> Polygon2 {
    let mut output = Polygon2::new();
    let Some(mut previous) = polygon.last().copied() else {
        return output;
    };
    let mut previous_value = half_plane_value(previous, normal, threshold, retain_above);
    for &current in polygon {
        let current_value = half_plane_value(current, normal, threshold, retain_above);
        if (current_value >= 0.0) != (previous_value >= 0.0) {
            let denominator = previous_value - current_value;
            if denominator.abs() > PROJECTION_EPSILON {
                let fraction = previous_value / denominator;
                output.push(Point2 {
                    x: previous.x + (current.x - previous.x) * fraction,
                    y: previous.y + (current.y - previous.y) * fraction,
                });
            }
        }
        if current_value >= 0.0 {
            output.push(current);
        }
        previous = current;
        previous_value = current_value;
    }
    output
}

fn half_plane_value(point: Point2, normal: Point2, threshold: f32, retain_above: bool) -> f32 {
    let value = dot2(point, normal) - threshold;
    if retain_above {
        value
    } else {
        -value
    }
}

fn is_inside(point: Point2, edge_start: Point2, edge_end: Point2) -> bool {
    cross2(subtract(edge_end, edge_start), subtract(point, edge_start)) >= -PROJECTION_EPSILON
}

fn line_intersection(
    segment_start: Point2,
    segment_end: Point2,
    edge_start: Point2,
    edge_end: Point2,
) -> Option<Point2> {
    let segment = subtract(segment_end, segment_start);
    let edge = subtract(edge_end, edge_start);
    let denominator = cross2(segment, edge);
    let scale = (dot2(segment, segment) * dot2(edge, edge)).sqrt();
    // Parallelism is dimensionless. An absolute epsilon rejects legitimate
    // intersections for tiny material fragments whose coordinate-scale cross
    // product is below 1e-6.
    if !denominator.is_finite()
        || !scale.is_finite()
        || denominator.abs() <= 16.0 * f32::EPSILON * scale
    {
        return None;
    }
    let fraction = cross2(subtract(edge_start, segment_start), edge) / denominator;
    Some(Point2 {
        x: segment_start.x + segment.x * fraction,
        y: segment_start.y + segment.y * fraction,
    })
}

fn signed_area(polygon: &[Point2]) -> f32 {
    if polygon.len() < 3 {
        return 0.0;
    }
    0.5 * polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f32>()
}

fn polygon_centroid(polygon: &[Point2]) -> Option<Point2> {
    if polygon.len() < 3 {
        return None;
    }
    let mut cross_sum = 0.0_f32;
    let mut x_sum = 0.0_f32;
    let mut y_sum = 0.0_f32;
    for (a, b) in polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
    {
        let cross = a.x * b.y - b.x * a.y;
        cross_sum += cross;
        x_sum += (a.x + b.x) * cross;
        y_sum += (a.y + b.y) * cross;
    }
    if !cross_sum.is_finite() || cross_sum == 0.0 {
        return None;
    }
    Some(Point2 {
        x: x_sum / (3.0 * cross_sum),
        y: y_sum / (3.0 * cross_sum),
    })
}

fn subtract(a: Point2, b: Point2) -> Point2 {
    Point2 {
        x: a.x - b.x,
        y: a.y - b.y,
    }
}

fn cross2(a: Point2, b: Point2) -> f32 {
    a.x * b.y - a.y * b.x
}

fn dot2(a: Point2, b: Point2) -> f32 {
    a.x * b.x + a.y * b.y
}

fn normalized(value: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot(value, value).sqrt();
    if !length.is_finite() || length <= PROJECTION_EPSILON {
        return None;
    }
    Some([value[0] / length, value[1] / length, value[2] / length])
}

fn cell_neighbors<'a>(cell: usize, nbr_offsets: &[u32], nbrs: &'a [u32]) -> Option<&'a [u32]> {
    let start = *nbr_offsets.get(cell)? as usize;
    let end = *nbr_offsets.get(cell + 1)? as usize;
    nbrs.get(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::mesh::{build_neighbors, generate_icosphere};
    use crate::sim::geology_types::CrustType;

    fn plate_state(axis: [f32; 3], speed: f32) -> PlateKinematicsState {
        PlateKinematicsState {
            angular_axis: axis,
            angular_speed: speed,
            reference_angular_speed: speed,
            slab_pull_drive: 0.0,
            ridge_push_drive: 0.0,
            collision_drag: 0.0,
            force_target_speed_km_per_myr: 0.0,
            basal_target_speed_km_per_myr: 0.0,
            phase_offset: 0.0,
            activity: 0.0,
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

    fn run_uniform_plate(level: u32, speed: f32) -> DualCellRemap {
        let (positions, indices) = generate_icosphere(level);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        remap_dual_cell_material(DualCellRemapInput {
            positions: &positions,
            nbr_offsets: &nbr_offsets,
            nbrs: &nbrs,
            plate_id: &vec![PlateId(0); positions.len()],
            crust: &vec![crust(); positions.len()],
            plate_states: &[plate_state([0.3, 0.7, -0.2], speed)],
            source_material: None,
        })
    }

    #[test]
    fn identity_remap_is_one_to_one_and_conservative() {
        let remap = run_uniform_plate(2, 0.0);

        assert_eq!(remap.diagnostics.unassigned_source_cell_count, 0);
        assert_eq!(remap.diagnostics.invalid_source_cell_count, 0);
        assert_eq!(remap.projection.diagnostics.uncovered_cell_count, 0);
        assert_eq!(remap.projection.diagnostics.mixed_plate_cell_count, 0);
        assert!(remap.projection.diagnostics.mass_conservation_error < 1e-3);
    }

    #[test]
    fn rigid_rotation_preserves_full_sphere_coverage_and_mass() {
        let remap = run_uniform_plate(3, 0.17);

        assert_eq!(remap.diagnostics.unassigned_source_cell_count, 0);
        assert_eq!(remap.diagnostics.invalid_source_cell_count, 0);
        assert_eq!(remap.projection.diagnostics.uncovered_cell_count, 0);
        assert!(remap.projection.diagnostics.mass_conservation_error < 1e-2);
    }

    #[test]
    fn persistent_mixture_preserves_per_plate_mass_under_common_rotation() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let source_material = vec![
            vec![
                SurfaceMaterialState {
                    plate_id: PlateId(0),
                    mass: 0.4,
                    oceanic_mass: 0.4,
                    age_mass: 8.0,
                },
                SurfaceMaterialState {
                    plate_id: PlateId(1),
                    mass: 0.6,
                    oceanic_mass: 0.0,
                    age_mass: 18.0,
                },
            ];
            positions.len()
        ];
        let states = [
            plate_state([0.3, 0.7, -0.2], 0.08),
            plate_state([0.3, 0.7, -0.2], 0.08),
        ];
        let remap = remap_dual_cell_material(DualCellRemapInput {
            positions: &positions,
            nbr_offsets: &nbr_offsets,
            nbrs: &nbrs,
            plate_id: &vec![PlateId(0); positions.len()],
            crust: &vec![crust(); positions.len()],
            plate_states: &states,
            source_material: Some(&source_material),
        });
        let mut mass = [0.0_f32; 2];
        for material in remap.projection.cells.iter().flatten() {
            mass[material.plate_id.as_usize()] += material.mass;
        }

        assert_eq!(remap.diagnostics.unassigned_source_cell_count, 0);
        assert_eq!(remap.diagnostics.invalid_source_cell_count, 0);
        assert_eq!(remap.projection.diagnostics.uncovered_cell_count, 0);
        assert!((mass[0] - positions.len() as f32 * 0.4).abs() < 1e-2);
        assert!((mass[1] - positions.len() as f32 * 0.6).abs() < 1e-2);
    }

    #[test]
    fn clipping_returns_subject_area_for_identical_polygons() {
        let polygon = vec![
            Point2 { x: -1.0, y: -1.0 },
            Point2 { x: 1.0, y: -1.0 },
            Point2 { x: 1.0, y: 1.0 },
            Point2 { x: -1.0, y: 1.0 },
        ];

        assert!((clipped_polygon_area(&polygon, &polygon) - 4.0).abs() < 1e-6);
    }

    #[test]
    fn clipping_keeps_intersection_for_a_tiny_non_parallel_segment() {
        let intersection = line_intersection(
            Point2 { x: 0.0, y: -2e-6 },
            Point2 { x: 0.0, y: 2e-6 },
            Point2 { x: -0.01, y: 0.0 },
            Point2 { x: 0.01, y: 0.0 },
        )
        .expect("tiny non-parallel segments must intersect");

        assert!(intersection.x.abs() < 1e-8);
        assert!(intersection.y.abs() < 1e-8);
    }

    #[test]
    fn spherical_area_cut_creates_one_shared_partition() {
        let center = normalized([1.0, 0.0, 0.0]).unwrap();
        let polygon = vec![
            normalized([1.0, -0.1, -0.1]).unwrap(),
            normalized([1.0, 0.1, -0.1]).unwrap(),
            normalized([1.0, 0.1, 0.1]).unwrap(),
            normalized([1.0, -0.1, 0.1]).unwrap(),
        ];
        let toward = normalized([1.0, 0.08, 0.0]).unwrap();

        let (retained, remainder) =
            cut_spherical_polygon_by_area_fraction(&polygon, center, toward, 0.35).unwrap();
        let frame = GnomonicFrame::new(center).unwrap();
        let full_spherical_area = spherical_polygon_area_vertices(&polygon).unwrap();
        let retained_spherical_area = spherical_polygon_area_vertices(&retained).unwrap();
        let full_projected_area = signed_area(&frame.project_polygon(&polygon).unwrap()).abs();
        let retained_projected_area = signed_area(&frame.project_polygon(&retained).unwrap()).abs();
        let remainder_projected_area =
            signed_area(&frame.project_polygon(&remainder).unwrap()).abs();

        assert!((retained_spherical_area / full_spherical_area - 0.35).abs() < 1e-5);
        assert!(
            (retained_projected_area + remainder_projected_area - full_projected_area).abs() < 1e-5
        );
        assert!(
            retained
                .iter()
                .map(|point| dot(*point, toward))
                .sum::<f32>()
                / retained.len() as f32
                > remainder
                    .iter()
                    .map(|point| dot(*point, toward))
                    .sum::<f32>()
                    / remainder.len() as f32
        );
    }
}
