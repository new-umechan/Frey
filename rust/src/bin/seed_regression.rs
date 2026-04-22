use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use frey_wasm::sim;
use frey_wasm::sim::erosion::ErosionAutomatonState;
use frey_wasm::sim::world::{FeedbackQueue, World, WorldMetrics};
use frey_wasm::GeologyParams;
use serde::{Deserialize, Serialize};

const DEFAULT_TICKS: u32 = 32;
const DEFAULT_THRESHOLD: f64 = 0.005;
const DEFAULT_LEVEL: u32 = 6;
const DEFAULT_JOBS: usize = 1;
const DEFAULT_SEEDS: [&str; 1] = ["alpha"];
const TRANSITION_MODE: &str = "fixed_tick";
const ERA_BOUNDARIES: [u32; 5] = [0, 800, 1300, 1395, 1445];
const METRIC_SPECS: [MetricSpec; 10] = [
    MetricSpec::new("land_cells", "land-cells"),
    MetricSpec::new("height_mean", "height-mean"),
    MetricSpec::new("height_std", "height-std"),
    MetricSpec::new("max_river_flux", "max-river-flux"),
    MetricSpec::new("top10_river_flux_sum", "top10-river-flux-sum"),
    MetricSpec::new("global_sediment_export", "global-sediment-export"),
    MetricSpec::new("marine_sediment_mass", "marine-sediment-mass"),
    MetricSpec::new(
        "solid_earth_mass_proxy_drift",
        "solid-earth-mass-proxy-drift",
    ),
    MetricSpec::new("ocean_water_inventory_drift", "ocean-water-inventory-drift"),
    MetricSpec::new("ice_inventory", "ice-inventory"),
];

#[derive(Clone, Copy)]
struct MetricSpec {
    key: &'static str,
    flag_suffix: &'static str,
}

impl MetricSpec {
    const fn new(key: &'static str, flag_suffix: &'static str) -> Self {
        Self { key, flag_suffix }
    }
}

#[derive(Clone)]
struct Args {
    ticks: u32,
    seeds: Vec<String>,
    jobs: usize,
    level: u32,
    out: Option<PathBuf>,
    baseline: Option<PathBuf>,
    check: bool,
    threshold: f64,
    threshold_by_metric: ThresholdMap,
    fail_on_deviation: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct OutputData {
    meta: OutputMeta,
    results: Vec<OutputEntry>,
}

#[derive(Clone, Serialize, Deserialize)]
struct OutputMeta {
    generated_at: String,
    ticks: u32,
    level: u32,
    seeds: Vec<String>,
    thresholds: ThresholdMap,
    transition_mode: String,
    era_boundaries: Vec<u32>,
    eras_at_measurement: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct OutputEntry {
    seed: String,
    tick: u32,
    era: String,
    metrics: MetricValues,
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct MetricValues {
    land_cells: f64,
    height_mean: f64,
    height_std: f64,
    max_river_flux: f64,
    top10_river_flux_sum: f64,
    global_sediment_export: f64,
    marine_sediment_mass: f64,
    solid_earth_mass_proxy_drift: f64,
    ocean_water_inventory_drift: f64,
    ice_inventory: f64,
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct ThresholdMap {
    land_cells: f64,
    height_mean: f64,
    height_std: f64,
    max_river_flux: f64,
    top10_river_flux_sum: f64,
    global_sediment_export: f64,
    marine_sediment_mass: f64,
    solid_earth_mass_proxy_drift: f64,
    ocean_water_inventory_drift: f64,
    ice_inventory: f64,
}

#[derive(Clone, Serialize, Deserialize)]
struct SimulationResult {
    seed: String,
    era: String,
    metrics: MetricValues,
}

#[derive(Clone)]
struct EvaluationResult {
    warnings: Vec<String>,
    deviations: Vec<Deviation>,
}

#[derive(Clone)]
struct Deviation {
    seed: String,
    metric: String,
    reason: Option<String>,
    mode: Option<&'static str>,
    current_value: Option<f64>,
    baseline_value: Option<f64>,
    diff: Option<f64>,
    threshold: Option<f64>,
    expected: Option<String>,
    actual: Option<String>,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut args = Args {
        ticks: DEFAULT_TICKS,
        seeds: DEFAULT_SEEDS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        jobs: DEFAULT_JOBS,
        level: DEFAULT_LEVEL,
        out: None,
        baseline: None,
        check: false,
        threshold: DEFAULT_THRESHOLD,
        threshold_by_metric: ThresholdMap::default(),
        fail_on_deviation: false,
    };

    let mut index = 0usize;
    while index < argv.len() {
        let token = &argv[index];
        if token == "--" {
            index += 1;
            continue;
        }

        if let Some(suffix) = token.strip_prefix("--threshold-") {
            if token == "--threshold" {
                return Err("internal parser error".to_string());
            }
            let Some(spec) = METRIC_SPECS
                .iter()
                .find(|candidate| candidate.flag_suffix == suffix)
            else {
                return Err(format!("Unknown argument: {token}"));
            };
            let value = parse_f64(next_arg(argv, index, token)?, token)?;
            set_threshold_by_key(&mut args.threshold_by_metric, spec.key, value.max(0.0))?;
            index += 2;
            continue;
        }

        match token.as_str() {
            "--ticks" => {
                args.ticks = parse_u32(next_arg(argv, index, token)?, token)?.max(1);
                index += 2;
            }
            "--seeds" => {
                let parsed = parse_seeds_csv(next_arg(argv, index, token)?);
                if parsed.is_empty() {
                    return Err("--seeds must include at least one seed".to_string());
                }
                args.seeds = parsed;
                index += 2;
            }
            "--jobs" => {
                args.jobs = parse_usize(next_arg(argv, index, token)?, token)?.max(1);
                index += 2;
            }
            "--level" => {
                args.level = parse_u32(next_arg(argv, index, token)?, token)?;
                index += 2;
            }
            "--out" => {
                args.out = Some(PathBuf::from(next_arg(argv, index, token)?));
                index += 2;
            }
            "--baseline" => {
                args.baseline = Some(PathBuf::from(next_arg(argv, index, token)?));
                index += 2;
            }
            "--check" => {
                args.check = true;
                index += 1;
            }
            "--threshold" => {
                args.threshold = parse_f64(next_arg(argv, index, token)?, token)?.max(0.0);
                index += 2;
            }
            "--fail-on-deviation" => {
                args.fail_on_deviation = true;
                index += 1;
            }
            "--help" => {
                print_help();
                std::process::exit(0);
            }
            _ => return Err(format!("Unknown argument: {token}")),
        }
    }

    if args.check && args.baseline.is_none() {
        return Err("--check requires --baseline <path>".to_string());
    }

    Ok(args)
}

fn print_help() {
    eprintln!(
        "Usage: cargo run --manifest-path rust/Cargo.toml --bin seed_regression -- [options]"
    );
    eprintln!("  --seeds <csv>");
    eprintln!("  --jobs <n>");
    eprintln!("  --ticks <n>");
    eprintln!("  --level <n>");
    eprintln!("  --out <path>");
    eprintln!("  --baseline <path>");
    eprintln!("  --check");
    eprintln!("  --threshold <ratio>");
    eprintln!("  --fail-on-deviation");
    for spec in METRIC_SPECS {
        eprintln!("  --threshold-{} <ratio>", spec.flag_suffix);
    }
}

fn next_arg<'a>(argv: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    argv.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_u32(value: &str, flag: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .map_err(|_| format!("{flag} must be a finite non-negative integer"))
}

fn parse_usize(value: &str, flag: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("{flag} must be a finite non-negative integer"))
}

fn parse_f64(value: &str, flag: &str) -> Result<f64, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| format!("{flag} must be a finite number"))?;
    if !parsed.is_finite() {
        return Err(format!("{flag} must be a finite number"));
    }
    Ok(parsed)
}

fn parse_seeds_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn build_effective_thresholds(args: &Args) -> ThresholdMap {
    ThresholdMap {
        land_cells: if args.threshold_by_metric.land_cells > 0.0 {
            args.threshold_by_metric.land_cells
        } else {
            args.threshold
        },
        height_mean: if args.threshold_by_metric.height_mean > 0.0 {
            args.threshold_by_metric.height_mean
        } else {
            args.threshold
        },
        height_std: if args.threshold_by_metric.height_std > 0.0 {
            args.threshold_by_metric.height_std
        } else {
            args.threshold
        },
        max_river_flux: if args.threshold_by_metric.max_river_flux > 0.0 {
            args.threshold_by_metric.max_river_flux
        } else {
            args.threshold
        },
        top10_river_flux_sum: if args.threshold_by_metric.top10_river_flux_sum > 0.0 {
            args.threshold_by_metric.top10_river_flux_sum
        } else {
            args.threshold
        },
        global_sediment_export: if args.threshold_by_metric.global_sediment_export > 0.0 {
            args.threshold_by_metric.global_sediment_export
        } else {
            args.threshold
        },
        marine_sediment_mass: if args.threshold_by_metric.marine_sediment_mass > 0.0 {
            args.threshold_by_metric.marine_sediment_mass
        } else {
            args.threshold
        },
        solid_earth_mass_proxy_drift: if args.threshold_by_metric.solid_earth_mass_proxy_drift > 0.0
        {
            args.threshold_by_metric.solid_earth_mass_proxy_drift
        } else {
            args.threshold
        },
        ocean_water_inventory_drift: if args.threshold_by_metric.ocean_water_inventory_drift > 0.0 {
            args.threshold_by_metric.ocean_water_inventory_drift
        } else {
            args.threshold
        },
        ice_inventory: if args.threshold_by_metric.ice_inventory > 0.0 {
            args.threshold_by_metric.ice_inventory
        } else {
            args.threshold
        },
    }
}

fn set_threshold_by_key(map: &mut ThresholdMap, key: &str, value: f64) -> Result<(), String> {
    match key {
        "land_cells" => map.land_cells = value,
        "height_mean" => map.height_mean = value,
        "height_std" => map.height_std = value,
        "max_river_flux" => map.max_river_flux = value,
        "top10_river_flux_sum" => map.top10_river_flux_sum = value,
        "global_sediment_export" => map.global_sediment_export = value,
        "marine_sediment_mass" => map.marine_sediment_mass = value,
        "solid_earth_mass_proxy_drift" => map.solid_earth_mass_proxy_drift = value,
        "ocean_water_inventory_drift" => map.ocean_water_inventory_drift = value,
        "ice_inventory" => map.ice_inventory = value,
        _ => return Err(format!("unsupported metric key: {key}")),
    }
    Ok(())
}

fn metric_value(metrics: &MetricValues, key: &str) -> Result<f64, String> {
    match key {
        "land_cells" => Ok(metrics.land_cells),
        "height_mean" => Ok(metrics.height_mean),
        "height_std" => Ok(metrics.height_std),
        "max_river_flux" => Ok(metrics.max_river_flux),
        "top10_river_flux_sum" => Ok(metrics.top10_river_flux_sum),
        "global_sediment_export" => Ok(metrics.global_sediment_export),
        "marine_sediment_mass" => Ok(metrics.marine_sediment_mass),
        "solid_earth_mass_proxy_drift" => Ok(metrics.solid_earth_mass_proxy_drift),
        "ocean_water_inventory_drift" => Ok(metrics.ocean_water_inventory_drift),
        "ice_inventory" => Ok(metrics.ice_inventory),
        _ => Err(format!("unsupported metric key: {key}")),
    }
}

fn threshold_value(thresholds: &ThresholdMap, key: &str) -> Result<f64, String> {
    match key {
        "land_cells" => Ok(thresholds.land_cells),
        "height_mean" => Ok(thresholds.height_mean),
        "height_std" => Ok(thresholds.height_std),
        "max_river_flux" => Ok(thresholds.max_river_flux),
        "top10_river_flux_sum" => Ok(thresholds.top10_river_flux_sum),
        "global_sediment_export" => Ok(thresholds.global_sediment_export),
        "marine_sediment_mass" => Ok(thresholds.marine_sediment_mass),
        "solid_earth_mass_proxy_drift" => Ok(thresholds.solid_earth_mass_proxy_drift),
        "ocean_water_inventory_drift" => Ok(thresholds.ocean_water_inventory_drift),
        "ice_inventory" => Ok(thresholds.ice_inventory),
        _ => Err(format!("unsupported metric key: {key}")),
    }
}

fn build_output(
    args: &Args,
    thresholds: ThresholdMap,
    results: Vec<SimulationResult>,
) -> OutputData {
    let mut eras_at_measurement = std::collections::BTreeMap::new();
    for result in &results {
        eras_at_measurement.insert(result.seed.clone(), result.era.clone());
    }

    OutputData {
        meta: OutputMeta {
            generated_at: iso8601_now_utc(),
            ticks: args.ticks,
            level: args.level,
            seeds: args.seeds.clone(),
            thresholds,
            transition_mode: TRANSITION_MODE.to_string(),
            era_boundaries: ERA_BOUNDARIES.to_vec(),
            eras_at_measurement,
        },
        results: results
            .into_iter()
            .map(|result| OutputEntry {
                seed: result.seed,
                tick: args.ticks,
                era: result.era,
                metrics: result.metrics,
            })
            .collect(),
    }
}

fn iso8601_now_utc() -> String {
    let output = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(result) if result.status.success() => {
            String::from_utf8_lossy(&result.stdout).trim().to_string()
        }
        _ => "1970-01-01T00:00:00Z".to_string(),
    }
}

fn run_seed_simulations(args: &Args) -> Result<Vec<SimulationResult>, String> {
    if args.jobs <= 1 || args.seeds.len() <= 1 {
        let mut results = Vec::with_capacity(args.seeds.len());
        for seed in &args.seeds {
            results.push(run_single_seed(seed, args.ticks, args.level)?);
        }
        return Ok(results);
    }

    let mut results = Vec::with_capacity(args.seeds.len());
    for seed in &args.seeds {
        results.push(run_single_seed_in_subprocess(seed, args)?);
    }
    Ok(results)
}

fn run_single_seed_in_subprocess(seed: &str, args: &Args) -> Result<SimulationResult, String> {
    let current_exe =
        env::current_exe().map_err(|err| format!("failed to resolve current exe: {err}"))?;
    let output = Command::new(current_exe)
        .args([
            "--seeds",
            seed,
            "--ticks",
            &args.ticks.to_string(),
            "--level",
            &args.level.to_string(),
            "--jobs",
            "1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| format!("failed to start subprocess for seed={seed}: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let details = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            "no output".to_string()
        };
        return Err(format!("subprocess failed for seed={seed}: {details}"));
    }

    let parsed: OutputData = serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("failed to parse subprocess output for seed={seed}: {err}"))?;
    let Some(entry) = parsed
        .results
        .into_iter()
        .find(|candidate| candidate.seed == seed)
    else {
        return Err(format!(
            "invalid subprocess output for seed={seed}: missing seed result"
        ));
    };
    Ok(SimulationResult {
        seed: entry.seed,
        era: entry.era,
        metrics: entry.metrics,
    })
}

fn run_single_seed(seed: &str, ticks: u32, level: u32) -> Result<SimulationResult, String> {
    let geology_params = GeologyParams {
        level,
        ..GeologyParams::default()
    };
    let (mut world, erosion_state) = frey_wasm::sim::headless::init_world_for_headless_runner(
        seed,
        level,
        geology_params.clone(),
    )?;
    let mut hydrology_state = Some(erosion_state);
    let mut feedback = FeedbackQueue::new(world.cell_count());

    for _ in 0..ticks {
        sim::exec_world_with_feedback_and_hydrology(
            &mut world,
            &mut feedback,
            &mut hydrology_state,
        );
        post_step_sync_light(&mut world, hydrology_state.as_mut(), &geology_params);
    }

    let metrics = world.metrics();
    Ok(SimulationResult {
        seed: seed.to_string(),
        era: world.clock.epoch.as_key().to_string(),
        metrics: collect_metrics(&metrics),
    })
}

fn post_step_sync_light(
    world: &mut World,
    hydrology_state: Option<&mut ErosionAutomatonState>,
    params: &GeologyParams,
) {
    let Some(state) = hydrology_state else {
        return;
    };
    frey_wasm::sim::hydrology::sync_hydrology_state_for_headless_runner(world, state, params);
}

fn collect_metrics(metrics: &WorldMetrics) -> MetricValues {
    MetricValues {
        land_cells: metrics.land_cells as f64,
        height_mean: metrics.mean_height as f64,
        height_std: metrics.height_std_dev as f64,
        max_river_flux: metrics.max_river_flux as f64,
        top10_river_flux_sum: metrics.top10_river_flux_sum as f64,
        global_sediment_export: metrics.global_sediment_export as f64,
        marine_sediment_mass: metrics.marine_sediment_mass as f64,
        solid_earth_mass_proxy_drift: metrics.solid_earth_mass_proxy_drift as f64,
        ocean_water_inventory_drift: metrics.ocean_water_inventory_drift as f64,
        ice_inventory: metrics.ice_inventory as f64,
    }
}

fn load_baseline(path: &PathBuf) -> Result<OutputData, String> {
    let content = fs::read_to_string(path)
        .map_err(|err| format!("failed to read baseline {}: {err}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|err| format!("failed to parse baseline {}: {err}", path.display()))
}

fn evaluate_against_baseline(
    current: &OutputData,
    baseline: &OutputData,
    thresholds: &ThresholdMap,
) -> Result<EvaluationResult, String> {
    let mut warnings = Vec::new();
    let mut deviations = validate_baseline_meta(current, baseline);
    if !deviations.is_empty() {
        return Ok(EvaluationResult {
            warnings,
            deviations,
        });
    }

    let current_by_seed = current
        .results
        .iter()
        .map(|entry| (entry.seed.as_str(), &entry.metrics))
        .collect::<std::collections::BTreeMap<_, _>>();
    let baseline_by_seed = baseline
        .results
        .iter()
        .map(|entry| (entry.seed.as_str(), &entry.metrics))
        .collect::<std::collections::BTreeMap<_, _>>();

    for seed in &current.meta.seeds {
        let Some(current_metrics) = current_by_seed.get(seed.as_str()) else {
            deviations.push(Deviation {
                seed: seed.clone(),
                metric: "*".to_string(),
                reason: Some("missing_seed_in_current".to_string()),
                mode: None,
                current_value: None,
                baseline_value: None,
                diff: None,
                threshold: None,
                expected: None,
                actual: None,
            });
            continue;
        };
        let Some(baseline_metrics) = baseline_by_seed.get(seed.as_str()) else {
            deviations.push(Deviation {
                seed: seed.clone(),
                metric: "*".to_string(),
                reason: Some("missing_seed_in_baseline".to_string()),
                mode: None,
                current_value: None,
                baseline_value: None,
                diff: None,
                threshold: None,
                expected: None,
                actual: None,
            });
            continue;
        };

        for spec in METRIC_SPECS {
            let current_value = metric_value(current_metrics, spec.key)?;
            let baseline_value = metric_value(baseline_metrics, spec.key)?;
            let threshold = threshold_value(thresholds, spec.key)?;
            let (mode, diff) = relative_or_absolute_diff(current_value, baseline_value);
            if diff > threshold {
                deviations.push(Deviation {
                    seed: seed.clone(),
                    metric: spec.key.to_string(),
                    reason: None,
                    mode: Some(mode),
                    current_value: Some(current_value),
                    baseline_value: Some(baseline_value),
                    diff: Some(diff),
                    threshold: Some(threshold),
                    expected: None,
                    actual: None,
                });
            }
        }
    }

    for seed in baseline_by_seed.keys() {
        if !current_by_seed.contains_key(seed) {
            warnings.push(format!(
                "baseline has extra seed not in current result: {seed}"
            ));
        }
    }

    Ok(EvaluationResult {
        warnings,
        deviations,
    })
}

fn validate_baseline_meta(current: &OutputData, baseline: &OutputData) -> Vec<Deviation> {
    let mut failures = Vec::new();

    if baseline.meta.ticks != current.meta.ticks {
        failures.push(meta_deviation(
            "meta.ticks",
            current.meta.ticks.to_string(),
            baseline.meta.ticks.to_string(),
        ));
    }
    if baseline.meta.level != current.meta.level {
        failures.push(meta_deviation(
            "meta.level",
            current.meta.level.to_string(),
            baseline.meta.level.to_string(),
        ));
    }

    let mut current_seeds = current.meta.seeds.clone();
    let mut baseline_seeds = baseline.meta.seeds.clone();
    current_seeds.sort();
    baseline_seeds.sort();
    if current_seeds != baseline_seeds {
        failures.push(meta_deviation(
            "meta.seeds",
            current_seeds.join(","),
            baseline_seeds.join(","),
        ));
    }

    if baseline.meta.transition_mode != current.meta.transition_mode {
        failures.push(meta_deviation(
            "meta.transition_mode",
            current.meta.transition_mode.clone(),
            baseline.meta.transition_mode.clone(),
        ));
    }

    if baseline.meta.era_boundaries != current.meta.era_boundaries {
        failures.push(meta_deviation(
            "meta.era_boundaries",
            format!("{:?}", current.meta.era_boundaries),
            format!("{:?}", baseline.meta.era_boundaries),
        ));
    }

    for seed in &current.meta.seeds {
        let current_era = current
            .meta
            .eras_at_measurement
            .get(seed)
            .cloned()
            .unwrap_or_default();
        let baseline_era = baseline
            .meta
            .eras_at_measurement
            .get(seed)
            .cloned()
            .unwrap_or_default();
        if current_era != baseline_era {
            failures.push(Deviation {
                seed: seed.clone(),
                metric: "meta.eras_at_measurement".to_string(),
                reason: Some("baseline_meta_mismatch".to_string()),
                mode: None,
                current_value: None,
                baseline_value: None,
                diff: None,
                threshold: None,
                expected: Some(current_era),
                actual: Some(baseline_era),
            });
        }
    }

    failures
}

fn meta_deviation(metric: &str, expected: String, actual: String) -> Deviation {
    Deviation {
        seed: "*".to_string(),
        metric: metric.to_string(),
        reason: Some("baseline_meta_mismatch".to_string()),
        mode: None,
        current_value: None,
        baseline_value: None,
        diff: None,
        threshold: None,
        expected: Some(expected),
        actual: Some(actual),
    }
}

fn relative_or_absolute_diff(current_value: f64, baseline_value: f64) -> (&'static str, f64) {
    let abs_diff = (current_value - baseline_value).abs();
    if baseline_value == 0.0 {
        ("absolute", abs_diff)
    } else {
        ("relative", abs_diff / baseline_value.abs())
    }
}

fn write_output(path: &PathBuf, output: &str) -> Result<(), String> {
    fs::write(path, output).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn main() {
    if let Err(message) = run() {
        let _ = writeln_stderr(&message);
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let argv = env::args().skip(1).collect::<Vec<_>>();
    let args = parse_args(&argv)?;
    let results = run_seed_simulations(&args)?;
    let thresholds = build_effective_thresholds(&args);
    let output_data = build_output(&args, thresholds.clone(), results);
    let output = serde_json::to_string_pretty(&output_data)
        .map_err(|err| format!("failed to serialize output: {err}"))?;
    println!("{output}");

    if let Some(path) = &args.out {
        write_output(path, &format!("{output}\n"))?;
    }

    if args.check {
        let baseline = load_baseline(
            args.baseline
                .as_ref()
                .ok_or_else(|| "--check requires --baseline <path>".to_string())?,
        )?;
        let comparison = evaluate_against_baseline(&output_data, &baseline, &thresholds)?;
        for warning in &comparison.warnings {
            eprintln!("[seed-regression] warn: {warning}");
        }
        for deviation in &comparison.deviations {
            if let Some(reason) = &deviation.reason {
                let expected = deviation
                    .expected
                    .as_ref()
                    .map(|value| format!(" expected={value}"))
                    .unwrap_or_default();
                let actual = deviation
                    .actual
                    .as_ref()
                    .map(|value| format!(" actual={value}"))
                    .unwrap_or_default();
                eprintln!(
                    "[seed-regression] deviation seed={} metric={} reason={}{}{}",
                    deviation.seed, deviation.metric, reason, expected, actual
                );
                continue;
            }
            eprintln!(
                "[seed-regression] deviation seed={} metric={} mode={} current={} baseline={} diff={} threshold={}",
                deviation.seed,
                deviation.metric,
                deviation.mode.unwrap_or("relative"),
                deviation.current_value.unwrap_or(0.0),
                deviation.baseline_value.unwrap_or(0.0),
                deviation.diff.unwrap_or(0.0),
                deviation.threshold.unwrap_or(0.0)
            );
        }
        eprintln!(
            "[seed-regression] deviations={}",
            comparison.deviations.len()
        );
        if args.fail_on_deviation && !comparison.deviations.is_empty() {
            return Err("seed regression deviations detected".to_string());
        }
    }

    Ok(())
}

fn writeln_stderr(message: &str) -> io::Result<()> {
    use std::io::Write;
    let mut stderr = io::stderr().lock();
    writeln!(stderr, "{message}")
}
