use crate::sim::exec::math::{cross3, dot};

const GEOMETRY_EPSILON: f64 = 1e-12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TriangleCutPartition {
    pub left_fraction: f64,
    pub right_fraction: f64,
}

pub(super) fn partition_triangle_by_shifted_cut(
    triangle: [[f32; 3]; 3],
    cut: [[f32; 3]; 2],
    signed_normal_displacement: f64,
) -> Option<TriangleCutPartition> {
    let center = normalized3([
        triangle[0][0] + triangle[1][0] + triangle[2][0],
        triangle[0][1] + triangle[1][1] + triangle[2][1],
        triangle[0][2] + triangle[1][2] + triangle[2][2],
    ])?;
    let frame = TangentFrame::new(center)?;
    let triangle_2d = triangle
        .map(|point| frame.project(point))
        .into_iter()
        .collect::<Option<Vec<_>>>()?;
    let cut_2d = cut.map(|point| frame.project(point));
    let [Some(start), Some(end)] = cut_2d else {
        return None;
    };
    let direction = Point2 {
        x: end.x - start.x,
        y: end.y - start.y,
    };
    let length = (direction.x * direction.x + direction.y * direction.y).sqrt();
    if !length.is_finite() || length <= GEOMETRY_EPSILON {
        return None;
    }
    let left_normal = Point2 {
        x: -direction.y / length,
        y: direction.x / length,
    };
    let threshold = dot2(start, left_normal) + signed_normal_displacement;
    let left_polygon = clip_half_plane(&triangle_2d, left_normal, threshold, true);
    let right_polygon = clip_half_plane(&triangle_2d, left_normal, threshold, false);
    let total_area = polygon_area(&triangle_2d);
    if !total_area.is_finite() || total_area <= GEOMETRY_EPSILON {
        return None;
    }
    Some(TriangleCutPartition {
        left_fraction: (polygon_area(&left_polygon) / total_area).clamp(0.0, 1.0),
        right_fraction: (polygon_area(&right_polygon) / total_area).clamp(0.0, 1.0),
    })
}

#[derive(Clone, Copy)]
struct Point2 {
    x: f64,
    y: f64,
}

struct TangentFrame {
    center: [f32; 3],
    tangent: [f32; 3],
    bitangent: [f32; 3],
}

impl TangentFrame {
    fn new(center: [f32; 3]) -> Option<Self> {
        let seed = if center[1].abs() < 0.95 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let tangent = normalized3(cross3(seed, center))?;
        let bitangent = normalized3(cross3(center, tangent))?;
        Some(Self {
            center,
            tangent,
            bitangent,
        })
    }

    fn project(&self, point: [f32; 3]) -> Option<Point2> {
        let denominator = dot(point, self.center) as f64;
        if denominator <= GEOMETRY_EPSILON {
            return None;
        }
        Some(Point2 {
            x: dot(point, self.tangent) as f64 / denominator,
            y: dot(point, self.bitangent) as f64 / denominator,
        })
    }
}

fn clip_half_plane(
    polygon: &[Point2],
    normal: Point2,
    threshold: f64,
    retain_above: bool,
) -> Vec<Point2> {
    let mut output = Vec::new();
    let Some(mut previous) = polygon.last().copied() else {
        return output;
    };
    let mut previous_value = signed_half_plane(previous, normal, threshold, retain_above);
    for &current in polygon {
        let current_value = signed_half_plane(current, normal, threshold, retain_above);
        if (current_value >= 0.0) != (previous_value >= 0.0) {
            let denominator = previous_value - current_value;
            if denominator.abs() > GEOMETRY_EPSILON {
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

fn signed_half_plane(point: Point2, normal: Point2, threshold: f64, retain_above: bool) -> f64 {
    let value = dot2(point, normal) - threshold;
    if retain_above {
        value
    } else {
        -value
    }
}

fn polygon_area(polygon: &[Point2]) -> f64 {
    0.5 * polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum::<f64>()
        .abs()
}

fn dot2(a: Point2, b: Point2) -> f64 {
    a.x * b.x + a.y * b.y
}

fn normalized3(value: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot(value, value).sqrt();
    if !length.is_finite() || length <= GEOMETRY_EPSILON as f32 {
        return None;
    }
    Some([value[0] / length, value[1] / length, value[2] / length])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(y: f32, z: f32) -> [f32; 3] {
        normalized3([1.0, y, z]).unwrap()
    }

    #[test]
    fn shifted_cut_conserves_local_partition_area() {
        let triangle = [point(-0.1, -0.1), point(0.1, -0.1), point(0.0, 0.12)];
        let cut = [point(-0.055, 0.0), point(0.055, 0.0)];
        let before = partition_triangle_by_shifted_cut(triangle, cut, 0.0).unwrap();
        let after = partition_triangle_by_shifted_cut(triangle, cut, 0.005).unwrap();

        assert!((before.left_fraction + before.right_fraction - 1.0).abs() < 1e-10);
        assert!((after.left_fraction + after.right_fraction - 1.0).abs() < 1e-10);
        assert!(after.left_fraction < before.left_fraction);
        assert!(after.right_fraction > before.right_fraction);
        assert!(
            ((after.left_fraction - before.left_fraction)
                + (after.right_fraction - before.right_fraction))
                .abs()
                < 1e-10
        );
    }

    #[test]
    fn tangential_cut_motion_does_not_enter_normal_flux_kernel() {
        let triangle = [point(-0.1, -0.1), point(0.1, -0.1), point(0.0, 0.12)];
        let cut = [point(-0.055, 0.0), point(0.055, 0.0)];

        assert_eq!(
            partition_triangle_by_shifted_cut(triangle, cut, 0.0),
            partition_triangle_by_shifted_cut(triangle, cut, 0.0)
        );
    }
}
