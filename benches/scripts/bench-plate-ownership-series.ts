import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { spawn } from "node:child_process";

interface Args {
  seeds: string[];
  ticks: number;
  recordEvery: number;
  level: number;
  out: string;
  cargoManifest: string;
}

interface BenchRecord {
  run_id?: string;
  seed?: string;
  level?: number;
  samples: TickRecord[];
}

interface TickRecord {
  tick: number;
  plate_count: number;
  mean_boundary_complexity_growth?: number;
  max_boundary_complexity_growth?: number;
  persistent_boundary_complexity_growth_plate_ratio?: number;
  mean_plate_block_count?: number;
  max_plate_block_count?: number;
  multi_block_plate_ratio?: number;
  max_secondary_plate_block_ratio?: number;
  mean_weak_line_plate_block_count?: number;
  max_weak_line_plate_block_count?: number;
  weak_line_multi_block_plate_ratio?: number;
  max_secondary_weak_line_plate_block_ratio?: number;
  mean_euler_rotation_residual_ratio?: number;
  reciprocal_churn_ratio?: number;
  net_exchange_directionality_ratio?: number;
  mutual_exchange_ratio?: number;
  temporal_reversal_ratio?: number;
  persistent_branch_plate_ratio?: number;
  nearest_centroid_voronoi_agreement_ratio?: number;
  centroid_voronoi_energy_ratio_from_initial?: number;
  boundary_motion_expected_cell_count?: number;
  boundary_motion_actual_cell_count?: number;
  boundary_motion_response_ratio?: number;
  boundary_topology_event_cell_count?: number;
  boundary_topology_constrained_segment_count?: number;
  boundary_motion_underactive_risk?: number;
  boundary_motion_overactive_risk?: number;
  boundary_motion_runtime_raw_expected_cell_count?: number;
  boundary_motion_runtime_accumulated_expected_cell_count?: number;
  boundary_motion_runtime_component_budget_cell_count?: number;
  boundary_motion_runtime_transferable_component_budget_cell_count?: number;
  boundary_motion_runtime_plate_consistency_budget_cell_count?: number;
  boundary_motion_runtime_plate_consistency_deferred_cell_count?: number;
  boundary_motion_runtime_plate_consistency_donor_limited_cell_count?: number;
  boundary_motion_runtime_plate_consistency_outgoing_limited_cell_count?: number;
  boundary_motion_runtime_plate_consistency_incoming_limited_cell_count?: number;
  boundary_motion_runtime_plate_consistency_net_area_limited_cell_count?: number;
  boundary_motion_runtime_plate_consistency_max_projected_out_ratio?: number;
  boundary_motion_runtime_actual_transfer_cell_count?: number;
  boundary_motion_runtime_patch_rejected_component_count?: number;
  boundary_motion_runtime_patch_rejected_budget_cell_count?: number;
  boundary_motion_runtime_source_fragment_rejected_component_count?: number;
  boundary_motion_runtime_source_fragment_rejected_budget_cell_count?: number;
  boundary_motion_runtime_target_disconnected_rejected_component_count?: number;
  boundary_motion_runtime_target_disconnected_rejected_budget_cell_count?: number;
  boundary_motion_runtime_budget_utilization_ratio?: number;
  boundary_motion_runtime_plate_consistency_limited_ratio?: number;
  boundary_motion_runtime_component_limited_ratio?: number;
  material_reconstruction_hard_capacity_assigned_cell_count?: number;
  material_reconstruction_closure_assigned_cell_count?: number;
  material_reconstruction_rebalanced_cell_count?: number;
  material_reconstruction_capacity_mismatch_cell_count?: number;
  material_reconstruction_non_dominant_assignment_cell_count?: number;
  material_reconstruction_mean_assigned_confidence?: number;
  mean_abs_plate_area_delta_ratio?: number;
  max_abs_plate_area_delta_ratio?: number;
  max_plate_area_growth_from_initial?: number;
  max_enclosed_plate_risk?: number;
  max_appendage_isolation_risk?: number;
}

interface GateRow {
  seed: string;
  current: TickRecord;
}

const DEFAULT_SEEDS = ["alpha", "beta", "gamma", "delta"];

function parseArgs(argv: string[]): Args {
  const args: Args = {
    seeds: DEFAULT_SEEDS,
    ticks: 160,
    recordEvery: 1,
    level: 6,
    out: "benches/results/plate_ownership_series.jsonl",
    cargoManifest: "rust/Cargo.toml",
  };
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    const next = argv[i + 1];
    switch (token) {
      case "--":
        break;
      case "--seeds":
        args.seeds = String(next ?? "")
          .split(",")
          .map((seed) => seed.trim())
          .filter(Boolean);
        i += 1;
        break;
      case "--ticks":
        args.ticks = Math.max(1, Number(next ?? args.ticks));
        i += 1;
        break;
      case "--record-every":
        args.recordEvery = Math.max(1, Number(next ?? args.recordEvery));
        i += 1;
        break;
      case "--level":
        args.level = Math.max(0, Number(next ?? args.level));
        i += 1;
        break;
      case "--out":
        args.out = String(next ?? args.out);
        i += 1;
        break;
      case "--cargo-manifest":
        args.cargoManifest = String(next ?? args.cargoManifest);
        i += 1;
        break;
      case "--help":
        printHelp();
        process.exit(0);
        break;
      default:
        throw new Error(`Unknown argument: ${token}`);
    }
  }
  if (args.seeds.length === 0) {
    throw new Error("--seeds must include at least one seed");
  }
  return args;
}

function printHelp() {
  console.error(
    "Usage: tsx benches/scripts/bench-plate-ownership-series.ts [options]",
  );
  console.error("  --seeds <csv>          default: alpha,beta,gamma,delta");
  console.error("  --ticks <n>            default: 160");
  console.error("  --record-every <n>     default: 1");
  console.error("  --level <n>            default: 6");
  console.error(
    "  --out <path>           default: benches/results/plate_ownership_series.jsonl",
  );
  console.error("  --cargo-manifest <path>");
}

function runOne(args: Args, seed: string, runId: string): Promise<void> {
  return new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(
      "cargo",
      [
        "run",
        "--manifest-path",
        args.cargoManifest,
        "--bin",
        "crust_plate_count_series",
      ],
      {
        stdio: "inherit",
        env: {
          ...process.env,
          CRUST_PLATE_SERIES_SEED: seed,
          CRUST_PLATE_SERIES_LEVEL: String(args.level),
          CRUST_PLATE_SERIES_TICKS: String(args.ticks),
          CRUST_PLATE_SERIES_RECORD_EVERY: String(args.recordEvery),
          CRUST_PLATE_SERIES_BENCH_OUT: args.out,
          CRUST_PLATE_SERIES_RUN_ID: `${runId}-${seed}`,
        },
      },
    );
    child.on("error", rejectPromise);
    child.on("exit", (code, signal) => {
      if (signal) {
        rejectPromise(
          new Error(`plate ownership run terminated by signal: ${signal}`),
        );
        return;
      }
      if (code !== 0) {
        rejectPromise(
          new Error(`plate ownership run failed with exit code ${code}`),
        );
        return;
      }
      resolvePromise();
    });
  });
}

async function loadRecords(pathname: string): Promise<BenchRecord[]> {
  const content = await readFile(resolve(pathname), "utf8");
  return content
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => JSON.parse(line) as BenchRecord)
    .filter((record) => Array.isArray(record.samples));
}

function findSample(record: BenchRecord, tick: number): TickRecord {
  const sample = record.samples.find((candidate) => candidate.tick === tick);
  if (!sample) {
    throw new Error(
      `No tick=${tick} sample for run_id=${record.run_id ?? "unknown"}`,
    );
  }
  return sample;
}

function latestRecord(records: BenchRecord[], runId: string): BenchRecord {
  const matches = records.filter((record) => record.run_id === runId);
  if (matches.length === 0) {
    throw new Error(`No record found for run_id=${runId}`);
  }
  return matches[matches.length - 1];
}

function finite(value: number | undefined): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function status(row: GateRow): "pass" | "warn" {
  const complexitySafe =
    finite(row.current.max_boundary_complexity_growth) <= 1.25;
  const persistenceSafe =
    finite(row.current.persistent_boundary_complexity_growth_plate_ratio) <=
    0.01;
  const areaGrowthSafe =
    finite(row.current.max_plate_area_growth_from_initial) <= 2.0;
  const areaDeltaSafe =
    finite(row.current.max_abs_plate_area_delta_ratio) <= 0.05;
  const enclosureSafe = finite(row.current.max_enclosed_plate_risk) <= 0.8;
  const temporalMotionSafe =
    finite(row.current.temporal_reversal_ratio) <= 0.05;
  const branchPersistenceSafe =
    finite(row.current.persistent_branch_plate_ratio) <= 0.0;
  const voronoiAttractorSafe =
    row.current.tick < 400 ||
    finite(row.current.centroid_voronoi_energy_ratio_from_initial) >= 0.75;
  const boundaryMotionSafe =
    finite(row.current.boundary_motion_underactive_risk) <= 0.0 &&
    finite(row.current.boundary_motion_overactive_risk) <= 0.0;
  return complexitySafe &&
    persistenceSafe &&
    areaGrowthSafe &&
    areaDeltaSafe &&
    enclosureSafe &&
    temporalMotionSafe &&
    branchPersistenceSafe &&
    voronoiAttractorSafe &&
    boundaryMotionSafe
    ? "pass"
    : "warn";
}

function fmt(value: number | undefined): string {
  return finite(value).toFixed(6);
}

function printRows(rows: GateRow[]) {
  console.log(
    "| seed | status | net_directionality | mutual_exchange | temporal_reversal | boundary_expected | boundary_actual | boundary_response | underactive | overactive | runtime_raw | runtime_accum | runtime_component_budget | runtime_transferable_budget | runtime_plate_consistency_budget | consistency_deferred | donor_limited | outgoing_limited | incoming_limited | net_area_limited | max_projected_out | runtime_actual | patch_reject_components | patch_reject_budget | source_fragment_reject_budget | target_disconnected_reject_budget | budget_utilization | plate_consistency_limited | component_limited | max_complexity | persistent | persistent_branch | voronoi_agreement | voronoi_energy_ratio | mean_blocks | max_blocks | multi_block | secondary_block | weak_mean_blocks | weak_max_blocks | weak_multi_block | weak_secondary_block | max_area_growth | max_area_delta | max_enclosed | max_appendage | mean_euler_residual |",
  );
  console.log(
    "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
  );
  for (const row of rows) {
    console.log(
      [
        `| ${row.seed}`,
        status(row),
        fmt(
          row.current.net_exchange_directionality_ratio ??
            row.current.reciprocal_churn_ratio,
        ),
        fmt(row.current.mutual_exchange_ratio),
        fmt(row.current.temporal_reversal_ratio),
        fmt(row.current.boundary_motion_expected_cell_count),
        fmt(row.current.boundary_motion_actual_cell_count),
        fmt(row.current.boundary_motion_response_ratio),
        fmt(row.current.boundary_motion_underactive_risk),
        fmt(row.current.boundary_motion_overactive_risk),
        fmt(row.current.boundary_motion_runtime_raw_expected_cell_count),
        fmt(
          row.current.boundary_motion_runtime_accumulated_expected_cell_count,
        ),
        fmt(row.current.boundary_motion_runtime_component_budget_cell_count),
        fmt(
          row.current
            .boundary_motion_runtime_transferable_component_budget_cell_count,
        ),
        fmt(
          row.current
            .boundary_motion_runtime_plate_consistency_budget_cell_count,
        ),
        fmt(
          row.current
            .boundary_motion_runtime_plate_consistency_deferred_cell_count,
        ),
        fmt(
          row.current
            .boundary_motion_runtime_plate_consistency_donor_limited_cell_count,
        ),
        fmt(
          row.current
            .boundary_motion_runtime_plate_consistency_outgoing_limited_cell_count,
        ),
        fmt(
          row.current
            .boundary_motion_runtime_plate_consistency_incoming_limited_cell_count,
        ),
        fmt(
          row.current
            .boundary_motion_runtime_plate_consistency_net_area_limited_cell_count,
        ),
        fmt(
          row.current
            .boundary_motion_runtime_plate_consistency_max_projected_out_ratio,
        ),
        fmt(row.current.boundary_motion_runtime_actual_transfer_cell_count),
        fmt(row.current.boundary_motion_runtime_patch_rejected_component_count),
        fmt(
          row.current.boundary_motion_runtime_patch_rejected_budget_cell_count,
        ),
        fmt(
          row.current
            .boundary_motion_runtime_source_fragment_rejected_budget_cell_count,
        ),
        fmt(
          row.current
            .boundary_motion_runtime_target_disconnected_rejected_budget_cell_count,
        ),
        fmt(row.current.boundary_motion_runtime_budget_utilization_ratio),
        fmt(
          row.current.boundary_motion_runtime_plate_consistency_limited_ratio,
        ),
        fmt(row.current.boundary_motion_runtime_component_limited_ratio),
        fmt(row.current.max_boundary_complexity_growth),
        fmt(row.current.persistent_boundary_complexity_growth_plate_ratio),
        fmt(row.current.persistent_branch_plate_ratio),
        fmt(row.current.nearest_centroid_voronoi_agreement_ratio),
        fmt(row.current.centroid_voronoi_energy_ratio_from_initial),
        fmt(row.current.mean_plate_block_count),
        fmt(row.current.max_plate_block_count),
        fmt(row.current.multi_block_plate_ratio),
        fmt(row.current.max_secondary_plate_block_ratio),
        fmt(row.current.mean_weak_line_plate_block_count),
        fmt(row.current.max_weak_line_plate_block_count),
        fmt(row.current.weak_line_multi_block_plate_ratio),
        fmt(row.current.max_secondary_weak_line_plate_block_ratio),
        fmt(row.current.max_plate_area_growth_from_initial),
        fmt(row.current.max_abs_plate_area_delta_ratio),
        fmt(row.current.max_enclosed_plate_risk),
        fmt(row.current.max_appendage_isolation_risk),
        fmt(row.current.mean_euler_rotation_residual_ratio),
        "|",
      ].join(" | "),
    );
  }
  const warnCount = rows.filter((row) => status(row) === "warn").length;
  console.log(`summary_total=${rows.length}`);
  console.log(`summary_warn=${warnCount}`);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const runId = `plate-ownership-${Date.now()}-${randomUUID().slice(0, 8)}`;
  for (const seed of args.seeds) {
    console.error(`[plate-ownership-series] seed=${seed} run_id=${runId}`);
    await runOne(args, seed, runId);
  }

  const records = await loadRecords(args.out);
  const rows = args.seeds.map((seed) => {
    const current = latestRecord(records, `${runId}-${seed}`);
    return {
      seed,
      current: findSample(current, args.ticks),
    };
  });
  console.log(`run_id=${runId}`);
  console.log(`ticks=${args.ticks}`);
  console.log(`record_every=${args.recordEvery}`);
  printRows(rows);
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
