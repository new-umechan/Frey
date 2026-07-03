use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};

use frey_wasm::sim;
use frey_wasm::sim::geology_types::GeologyParams;

const EARTH_PLATE_ID_MAGIC: &[u8; 8] = b"GEOPLID1";
const INVALID_PLATE_ID: u32 = u32::MAX;
const DEFAULT_TIMES_MA: [u32; 7] = [0, 10, 25, 50, 75, 100, 140];

#[derive(Debug, Clone)]
struct EarthPlateIdRef {
    time_ma: f32,
    plate_id: Vec<u32>,
}

#[derive(Debug, Clone)]
struct PlateShapeSummary {
    time_ma: f32,
    valid_cell_count: usize,
    unassigned_cell_count: usize,
    plate_count: usize,
    max_area_ratio: f32,
    effective_plate_count: f32,
    multi_component_plate_count: usize,
    max_component_count: usize,
    mean_detached_fragment_ratio: f32,
    max_detached_fragment_ratio: f32,
    mean_boundary_complexity: f32,
    max_boundary_complexity: f32,
    p95_boundary_complexity: f32,
    p99_boundary_complexity: f32,
    mean_elongation: f32,
    max_elongation: f32,
    p95_elongation: f32,
    p99_elongation: f32,
    mean_narrow_connection_cell_ratio: f32,
    max_narrow_connection_cell_ratio: f32,
    p95_narrow_connection_cell_ratio: f32,
    p99_narrow_connection_cell_ratio: f32,
    area_ge_1pct_plate_count: usize,
    area_ge_1pct_p95_boundary_complexity: f32,
    area_ge_1pct_p99_boundary_complexity: f32,
    area_ge_1pct_p95_elongation: f32,
    area_ge_1pct_p99_elongation: f32,
    area_ge_1pct_p95_narrow_connection_cell_ratio: f32,
    area_ge_1pct_p99_narrow_connection_cell_ratio: f32,
    top8_plate_count: usize,
    top8_p95_boundary_complexity: f32,
    top8_p99_boundary_complexity: f32,
    top8_p95_elongation: f32,
    top8_p99_elongation: f32,
    top8_p95_narrow_connection_cell_ratio: f32,
    top8_p99_narrow_connection_cell_ratio: f32,
}

#[derive(Debug, Clone)]
struct PlateMetricRow {
    area_ratio: f32,
    boundary_complexity: f32,
    elongation: f32,
    narrow_connection_cell_ratio: f32,
}

fn main() {
    let geology_params = GeologyParams {
        level: 6,
        ..Default::default()
    };
    let (_, positions, nbr_offsets, nbrs) =
        sim::build_geology_with_mesh("earth", geology_params);

    let mut summaries = Vec::new();
    for time_ma in DEFAULT_TIMES_MA {
        let filename = format!("earth_plate_id_ref_{time_ma:03}Ma.bin");
        let path = match find_data_file(&filename) {
            Some(path) => path,
            None => {
                println!("missing={filename}");
                continue;
            }
        };
        let plate_ref = match load_earth_plate_id_ref(&path) {
            Ok(value) => value,
            Err(error) => {
                println!("error={} {}", path.display(), error);
                continue;
            }
        };
        let summary = compute_plate_shape_summary(
            plate_ref.time_ma,
            &positions,
            &nbr_offsets,
            &nbrs,
            &plate_ref.plate_id,
        );
        print_summary(&summary);
        summaries.push(summary);
    }

    if summaries.is_empty() {
        println!("earth_plate_shape=SKIPPED no input files");
        return;
    }

    let output_path = results_path("earth_plate_shape_stats.json");
    if let Err(error) = write_summaries_json(&output_path, &summaries) {
        println!("earth_plate_shape_save=ERROR {}", error);
    } else {
        println!("earth_plate_shape_save=OK {}", output_path.display());
    }
}

fn find_data_file(filename: &str) -> Option<PathBuf> {
    let candidates = [
        Path::new("benches/data").join(filename),
        Path::new("data").join(filename),
        Path::new("../data").join(filename),
        Path::new("../../benches/data").join(filename),
    ];
    candidates.into_iter().find(|path| path.exists())
}

fn results_path(filename: &str) -> PathBuf {
    let candidates = [
        Path::new("benches/results").join(filename),
        Path::new("results").join(filename),
        Path::new("../results").join(filename),
        Path::new("../../benches/results").join(filename),
    ];
    for candidate in candidates {
        if let Some(parent) = candidate.parent() {
            if parent.exists() {
                return candidate;
            }
        }
    }
    Path::new("benches/results").join(filename)
}

fn load_earth_plate_id_ref(path: &Path) -> Result<EarthPlateIdRef, String> {
    let mut data = Vec::new();
    File::open(path)
        .map_err(|error| format!("failed to open: {error}"))?
        .read_to_end(&mut data)
        .map_err(|error| format!("failed to read: {error}"))?;
    if data.len() < 24 {
        return Err("file too short".to_string());
    }
    if &data[0..8] != EARTH_PLATE_ID_MAGIC {
        return Err("bad magic".to_string());
    }
    let version = u32::from_le_bytes(data[8..12].try_into().unwrap());
    if version != 1 {
        return Err(format!("unsupported version: {version}"));
    }
    let cell_count = u64::from_le_bytes(data[12..20].try_into().unwrap()) as usize;
    let time_ma = f32::from_le_bytes(data[20..24].try_into().unwrap());
    let expected_len = 24 + cell_count * 4;
    if data.len() != expected_len {
        return Err(format!(
            "length mismatch: expected {expected_len}, got {}",
            data.len()
        ));
    }
    let mut plate_id = Vec::with_capacity(cell_count);
    for chunk in data[24..].chunks_exact(4) {
        plate_id.push(u32::from_le_bytes(chunk.try_into().unwrap()));
    }
    Ok(EarthPlateIdRef { time_ma, plate_id })
}

fn compute_plate_shape_summary(
    time_ma: f32,
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
) -> PlateShapeSummary {
    let cell_count = plate_id
        .len()
        .min(positions.len())
        .min(nbr_offsets.len().saturating_sub(1));
    let valid_cell_count = plate_id
        .iter()
        .take(cell_count)
        .filter(|&&id| id != INVALID_PLATE_ID)
        .count();
    if cell_count == 0 || valid_cell_count == 0 {
        return empty_summary(time_ma, cell_count);
    }

    let plate_count = plate_id
        .iter()
        .take(cell_count)
        .filter(|&&id| id != INVALID_PLATE_ID)
        .map(|&id| id as usize)
        .max()
        .map(|max_id| max_id + 1)
        .unwrap_or(0);
    let mut cell_counts = vec![0usize; plate_count];
    let mut boundary_contacts = vec![0usize; plate_count];
    let mut narrow_cells = vec![0usize; plate_count];

    for i in 0..cell_count {
        let plate = plate_id[i];
        if plate == INVALID_PLATE_ID {
            continue;
        }
        let plate = plate as usize;
        cell_counts[plate] += 1;
        let mut same_plate_neighbors = 0usize;
        for &n_u32 in neighbors(nbr_offsets, nbrs, i) {
            let n = n_u32 as usize;
            if n >= cell_count {
                continue;
            }
            let other = plate_id[n];
            if other == INVALID_PLATE_ID {
                continue;
            }
            if other == plate_id[i] {
                same_plate_neighbors += 1;
            } else {
                boundary_contacts[plate] += 1;
            }
        }
        if same_plate_neighbors <= 2 {
            narrow_cells[plate] += 1;
        }
    }

    let mut component_counts = vec![0usize; plate_count];
    let mut largest_component_sizes = vec![0usize; plate_count];
    compute_components(
        nbr_offsets,
        nbrs,
        plate_id,
        cell_count,
        &mut component_counts,
        &mut largest_component_sizes,
    );

    let mut area_square_sum = 0.0_f32;
    let mut max_area_ratio = 0.0_f32;
    let mut multi_component_plate_count = 0usize;
    let mut max_component_count = 0usize;
    let mut detached_sum = 0.0_f32;
    let mut max_detached = 0.0_f32;
    let mut boundary_complexity_sum = 0.0_f32;
    let mut max_boundary_complexity = 0.0_f32;
    let mut boundary_complexity_values = Vec::new();
    let mut elongation_sum = 0.0_f32;
    let mut max_elongation = 0.0_f32;
    let mut elongation_values = Vec::new();
    let mut narrow_ratio_sum = 0.0_f32;
    let mut max_narrow_ratio = 0.0_f32;
    let mut narrow_ratio_values = Vec::new();
    let mut metric_rows = Vec::new();
    let mut populated_plate_count = 0usize;

    for plate in 0..plate_count {
        let cells = cell_counts[plate];
        if cells == 0 {
            continue;
        }
        populated_plate_count += 1;
        let area_ratio = cells as f32 / valid_cell_count as f32;
        max_area_ratio = max_area_ratio.max(area_ratio);
        area_square_sum += area_ratio * area_ratio;

        let components = component_counts[plate];
        max_component_count = max_component_count.max(components);
        if components > 1 {
            multi_component_plate_count += 1;
        }

        let detached = 1.0 - (largest_component_sizes[plate] as f32 / cells as f32);
        detached_sum += detached;
        max_detached = max_detached.max(detached);

        let boundary_complexity = boundary_contacts[plate] as f32 / (cells as f32).sqrt().max(1.0);
        boundary_complexity_sum += boundary_complexity;
        max_boundary_complexity = max_boundary_complexity.max(boundary_complexity);
        boundary_complexity_values.push(boundary_complexity);

        let elongation =
            approximate_plate_elongation(positions, plate_id, cell_count, plate as u32, cells);
        elongation_sum += elongation;
        max_elongation = max_elongation.max(elongation);
        elongation_values.push(elongation);

        let narrow_ratio = narrow_cells[plate] as f32 / cells as f32;
        narrow_ratio_sum += narrow_ratio;
        max_narrow_ratio = max_narrow_ratio.max(narrow_ratio);
        narrow_ratio_values.push(narrow_ratio);
        metric_rows.push(PlateMetricRow {
            area_ratio,
            boundary_complexity,
            elongation,
            narrow_connection_cell_ratio: narrow_ratio,
        });
    }

    let denom = populated_plate_count.max(1) as f32;
    let p95_boundary_complexity = percentile(&mut boundary_complexity_values.clone(), 0.95);
    let p99_boundary_complexity = percentile(&mut boundary_complexity_values, 0.99);
    let p95_elongation = percentile(&mut elongation_values.clone(), 0.95);
    let p99_elongation = percentile(&mut elongation_values, 0.99);
    let p95_narrow_connection_cell_ratio = percentile(&mut narrow_ratio_values.clone(), 0.95);
    let p99_narrow_connection_cell_ratio = percentile(&mut narrow_ratio_values, 0.99);
    let area_ge_1pct = scoped_percentiles(
        metric_rows
            .iter()
            .filter(|row| row.area_ratio >= 0.01)
            .cloned()
            .collect(),
    );
    let mut rows_by_area = metric_rows.clone();
    rows_by_area.sort_by(|left, right| {
        right
            .area_ratio
            .partial_cmp(&left.area_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let top8 = scoped_percentiles(rows_by_area.into_iter().take(8).collect());
    PlateShapeSummary {
        time_ma,
        valid_cell_count,
        unassigned_cell_count: cell_count.saturating_sub(valid_cell_count),
        plate_count: populated_plate_count,
        max_area_ratio,
        effective_plate_count: if area_square_sum > 0.0 {
            1.0 / area_square_sum
        } else {
            0.0
        },
        multi_component_plate_count,
        max_component_count,
        mean_detached_fragment_ratio: detached_sum / denom,
        max_detached_fragment_ratio: max_detached,
        mean_boundary_complexity: boundary_complexity_sum / denom,
        max_boundary_complexity,
        p95_boundary_complexity,
        p99_boundary_complexity,
        mean_elongation: elongation_sum / denom,
        max_elongation,
        p95_elongation,
        p99_elongation,
        mean_narrow_connection_cell_ratio: narrow_ratio_sum / denom,
        max_narrow_connection_cell_ratio: max_narrow_ratio,
        p95_narrow_connection_cell_ratio,
        p99_narrow_connection_cell_ratio,
        area_ge_1pct_plate_count: area_ge_1pct.plate_count,
        area_ge_1pct_p95_boundary_complexity: area_ge_1pct.p95_boundary_complexity,
        area_ge_1pct_p99_boundary_complexity: area_ge_1pct.p99_boundary_complexity,
        area_ge_1pct_p95_elongation: area_ge_1pct.p95_elongation,
        area_ge_1pct_p99_elongation: area_ge_1pct.p99_elongation,
        area_ge_1pct_p95_narrow_connection_cell_ratio: area_ge_1pct
            .p95_narrow_connection_cell_ratio,
        area_ge_1pct_p99_narrow_connection_cell_ratio: area_ge_1pct
            .p99_narrow_connection_cell_ratio,
        top8_plate_count: top8.plate_count,
        top8_p95_boundary_complexity: top8.p95_boundary_complexity,
        top8_p99_boundary_complexity: top8.p99_boundary_complexity,
        top8_p95_elongation: top8.p95_elongation,
        top8_p99_elongation: top8.p99_elongation,
        top8_p95_narrow_connection_cell_ratio: top8.p95_narrow_connection_cell_ratio,
        top8_p99_narrow_connection_cell_ratio: top8.p99_narrow_connection_cell_ratio,
    }
}

fn empty_summary(time_ma: f32, cell_count: usize) -> PlateShapeSummary {
    PlateShapeSummary {
        time_ma,
        valid_cell_count: 0,
        unassigned_cell_count: cell_count,
        plate_count: 0,
        max_area_ratio: 0.0,
        effective_plate_count: 0.0,
        multi_component_plate_count: 0,
        max_component_count: 0,
        mean_detached_fragment_ratio: 0.0,
        max_detached_fragment_ratio: 0.0,
        mean_boundary_complexity: 0.0,
        max_boundary_complexity: 0.0,
        p95_boundary_complexity: 0.0,
        p99_boundary_complexity: 0.0,
        mean_elongation: 0.0,
        max_elongation: 0.0,
        p95_elongation: 0.0,
        p99_elongation: 0.0,
        mean_narrow_connection_cell_ratio: 0.0,
        max_narrow_connection_cell_ratio: 0.0,
        p95_narrow_connection_cell_ratio: 0.0,
        p99_narrow_connection_cell_ratio: 0.0,
        area_ge_1pct_plate_count: 0,
        area_ge_1pct_p95_boundary_complexity: 0.0,
        area_ge_1pct_p99_boundary_complexity: 0.0,
        area_ge_1pct_p95_elongation: 0.0,
        area_ge_1pct_p99_elongation: 0.0,
        area_ge_1pct_p95_narrow_connection_cell_ratio: 0.0,
        area_ge_1pct_p99_narrow_connection_cell_ratio: 0.0,
        top8_plate_count: 0,
        top8_p95_boundary_complexity: 0.0,
        top8_p99_boundary_complexity: 0.0,
        top8_p95_elongation: 0.0,
        top8_p99_elongation: 0.0,
        top8_p95_narrow_connection_cell_ratio: 0.0,
        top8_p99_narrow_connection_cell_ratio: 0.0,
    }
}

#[derive(Debug, Clone)]
struct ScopedPercentiles {
    plate_count: usize,
    p95_boundary_complexity: f32,
    p99_boundary_complexity: f32,
    p95_elongation: f32,
    p99_elongation: f32,
    p95_narrow_connection_cell_ratio: f32,
    p99_narrow_connection_cell_ratio: f32,
}

fn scoped_percentiles(rows: Vec<PlateMetricRow>) -> ScopedPercentiles {
    let mut boundary = rows
        .iter()
        .map(|row| row.boundary_complexity)
        .collect::<Vec<_>>();
    let mut elongation = rows.iter().map(|row| row.elongation).collect::<Vec<_>>();
    let mut narrow = rows
        .iter()
        .map(|row| row.narrow_connection_cell_ratio)
        .collect::<Vec<_>>();
    ScopedPercentiles {
        plate_count: rows.len(),
        p95_boundary_complexity: percentile(&mut boundary.clone(), 0.95),
        p99_boundary_complexity: percentile(&mut boundary, 0.99),
        p95_elongation: percentile(&mut elongation.clone(), 0.95),
        p99_elongation: percentile(&mut elongation, 0.99),
        p95_narrow_connection_cell_ratio: percentile(&mut narrow.clone(), 0.95),
        p99_narrow_connection_cell_ratio: percentile(&mut narrow, 0.99),
    }
}

fn neighbors<'a>(nbr_offsets: &[u32], nbrs: &'a [u32], index: usize) -> &'a [u32] {
    let start = nbr_offsets.get(index).copied().unwrap_or(0) as usize;
    let end = nbr_offsets
        .get(index + 1)
        .copied()
        .unwrap_or(start as u32) as usize;
    nbrs.get(start..end).unwrap_or(&[])
}

fn compute_components(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[u32],
    cell_count: usize,
    component_counts: &mut [usize],
    largest_component_sizes: &mut [usize],
) {
    let mut visited = vec![false; cell_count];
    let mut queue = VecDeque::<usize>::new();

    for start in 0..cell_count {
        if visited[start] || plate_id[start] == INVALID_PLATE_ID {
            continue;
        }
        visited[start] = true;
        let plate = plate_id[start] as usize;
        if plate >= component_counts.len() {
            continue;
        }

        let mut size = 0usize;
        queue.push_back(start);
        while let Some(cell) = queue.pop_front() {
            size += 1;
            for &n_u32 in neighbors(nbr_offsets, nbrs, cell) {
                let n = n_u32 as usize;
                if n >= cell_count
                    || visited[n]
                    || plate_id[n] == INVALID_PLATE_ID
                    || plate_id[n] != plate_id[start]
                {
                    continue;
                }
                visited[n] = true;
                queue.push_back(n);
            }
        }

        component_counts[plate] += 1;
        largest_component_sizes[plate] = largest_component_sizes[plate].max(size);
    }
}

fn approximate_plate_elongation(
    positions: &[[f32; 3]],
    plate_id: &[u32],
    cell_count: usize,
    plate: u32,
    plate_cells: usize,
) -> f32 {
    let start = match plate_id
        .iter()
        .take(cell_count)
        .position(|&id| id == plate)
    {
        Some(index) => index,
        None => return 0.0,
    };
    let far = farthest_plate_cell(positions, plate_id, cell_count, plate, start).unwrap_or(start);
    let opposite = farthest_plate_cell(positions, plate_id, cell_count, plate, far).unwrap_or(far);
    let diameter = angular_distance(positions[far], positions[opposite]);
    let area_proxy = (plate_cells as f32 / cell_count as f32) * (4.0 * std::f32::consts::PI);
    diameter / area_proxy.sqrt().max(1e-6)
}

fn farthest_plate_cell(
    positions: &[[f32; 3]],
    plate_id: &[u32],
    cell_count: usize,
    plate: u32,
    from: usize,
) -> Option<usize> {
    let origin = *positions.get(from)?;
    let mut best = None::<usize>;
    let mut best_distance = -1.0_f32;
    for i in 0..cell_count {
        if plate_id[i] != plate {
            continue;
        }
        let distance = angular_distance(origin, positions[i]);
        if distance > best_distance {
            best_distance = distance;
            best = Some(i);
        }
    }
    best
}

fn angular_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dot = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0);
    dot.acos()
}

fn percentile(values: &mut [f32], q: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|left, right| {
        left.partial_cmp(right)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let rank = (values.len().saturating_sub(1) as f32) * q.clamp(0.0, 1.0);
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper || upper >= values.len() {
        return values[lower.min(values.len() - 1)];
    }
    let t = rank - lower as f32;
    values[lower] + (values[upper] - values[lower]) * t
}

fn print_summary(summary: &PlateShapeSummary) {
    println!(
        "time_ma={:.0} valid={} unassigned={} plates={} area_ge_1pct={} top8={} top8_p99_elongation={:.6} top8_p99_narrow_connection_cell_ratio={:.6} top8_p99_boundary_complexity={:.6}",
        summary.time_ma,
        summary.valid_cell_count,
        summary.unassigned_cell_count,
        summary.plate_count,
        summary.area_ge_1pct_plate_count,
        summary.top8_plate_count,
        summary.top8_p99_elongation,
        summary.top8_p99_narrow_connection_cell_ratio,
        summary.top8_p99_boundary_complexity,
    );
}

fn write_summaries_json(path: &Path, summaries: &[PlateShapeSummary]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {}", parent.display(), error))?;
    }
    let mut file = File::create(path)
        .map_err(|error| format!("failed to create {}: {}", path.display(), error))?;
    writeln!(file, "[").map_err(|error| error.to_string())?;
    for (index, summary) in summaries.iter().enumerate() {
        let comma = if index + 1 == summaries.len() { "" } else { "," };
        writeln!(
            file,
            "  {{\"time_ma\":{},\"valid_cell_count\":{},\"unassigned_cell_count\":{},\"plate_count\":{},\"max_area_ratio\":{:.6},\"effective_plate_count\":{:.6},\"multi_component_plate_count\":{},\"max_component_count\":{},\"mean_detached_fragment_ratio\":{:.6},\"max_detached_fragment_ratio\":{:.6},\"mean_boundary_complexity\":{:.6},\"max_boundary_complexity\":{:.6},\"p95_boundary_complexity\":{:.6},\"p99_boundary_complexity\":{:.6},\"mean_elongation\":{:.6},\"max_elongation\":{:.6},\"p95_elongation\":{:.6},\"p99_elongation\":{:.6},\"mean_narrow_connection_cell_ratio\":{:.6},\"max_narrow_connection_cell_ratio\":{:.6},\"p95_narrow_connection_cell_ratio\":{:.6},\"p99_narrow_connection_cell_ratio\":{:.6},\"area_ge_1pct_plate_count\":{},\"area_ge_1pct_p95_boundary_complexity\":{:.6},\"area_ge_1pct_p99_boundary_complexity\":{:.6},\"area_ge_1pct_p95_elongation\":{:.6},\"area_ge_1pct_p99_elongation\":{:.6},\"area_ge_1pct_p95_narrow_connection_cell_ratio\":{:.6},\"area_ge_1pct_p99_narrow_connection_cell_ratio\":{:.6},\"top8_plate_count\":{},\"top8_p95_boundary_complexity\":{:.6},\"top8_p99_boundary_complexity\":{:.6},\"top8_p95_elongation\":{:.6},\"top8_p99_elongation\":{:.6},\"top8_p95_narrow_connection_cell_ratio\":{:.6},\"top8_p99_narrow_connection_cell_ratio\":{:.6}}}{}",
            summary.time_ma,
            summary.valid_cell_count,
            summary.unassigned_cell_count,
            summary.plate_count,
            summary.max_area_ratio,
            summary.effective_plate_count,
            summary.multi_component_plate_count,
            summary.max_component_count,
            summary.mean_detached_fragment_ratio,
            summary.max_detached_fragment_ratio,
            summary.mean_boundary_complexity,
            summary.max_boundary_complexity,
            summary.p95_boundary_complexity,
            summary.p99_boundary_complexity,
            summary.mean_elongation,
            summary.max_elongation,
            summary.p95_elongation,
            summary.p99_elongation,
            summary.mean_narrow_connection_cell_ratio,
            summary.max_narrow_connection_cell_ratio,
            summary.p95_narrow_connection_cell_ratio,
            summary.p99_narrow_connection_cell_ratio,
            summary.area_ge_1pct_plate_count,
            summary.area_ge_1pct_p95_boundary_complexity,
            summary.area_ge_1pct_p99_boundary_complexity,
            summary.area_ge_1pct_p95_elongation,
            summary.area_ge_1pct_p99_elongation,
            summary.area_ge_1pct_p95_narrow_connection_cell_ratio,
            summary.area_ge_1pct_p99_narrow_connection_cell_ratio,
            summary.top8_plate_count,
            summary.top8_p95_boundary_complexity,
            summary.top8_p99_boundary_complexity,
            summary.top8_p95_elongation,
            summary.top8_p99_elongation,
            summary.top8_p95_narrow_connection_cell_ratio,
            summary.top8_p99_narrow_connection_cell_ratio,
            comma,
        )
        .map_err(|error| error.to_string())?;
    }
    writeln!(file, "]").map_err(|error| error.to_string())
}
