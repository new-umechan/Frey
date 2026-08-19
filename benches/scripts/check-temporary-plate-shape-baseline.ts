import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import process from "node:process";

interface Args {
  baseline: string;
  current?: string;
  cargoManifest: string;
}

interface MetricLimit {
  baseline: number;
  min?: number;
  max?: number;
}

interface BaselineSample {
  tick: number;
  metrics: Record<string, MetricLimit>;
}

interface Baseline {
  baseline_commit: string;
  seed: string;
  level: number;
  ticks: number;
  record_every: number;
  samples: BaselineSample[];
}

interface PlateRecord {
  component_count?: number;
  detached_fragment_ratio?: number;
}

interface TickRecord {
  tick: number;
  plates?: PlateRecord[];
  [key: string]: unknown;
}

interface BenchRecord {
  seed?: string;
  level?: number;
  ticks?: number;
  samples?: TickRecord[];
}

function parseArgs(argv: string[]): Args {
  const args: Args = {
    baseline: "tests/plate-shape/temporary-alpha-level6-baseline.json",
    cargoManifest: "rust/Cargo.toml",
  };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    const next = argv[index + 1];
    switch (token) {
      case "--baseline":
        args.baseline = String(next);
        index += 1;
        break;
      case "--current":
        args.current = String(next);
        index += 1;
        break;
      case "--cargo-manifest":
        args.cargoManifest = String(next);
        index += 1;
        break;
      default:
        throw new Error(`unknown argument: ${token}`);
    }
  }
  return args;
}

async function readJson<T>(pathname: string): Promise<T> {
  return JSON.parse(await readFile(resolve(pathname), "utf8")) as T;
}

async function readJsonLines(pathname: string): Promise<BenchRecord[]> {
  return (await readFile(resolve(pathname), "utf8"))
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => JSON.parse(line) as BenchRecord);
}

function runCurrent(
  baseline: Baseline,
  cargoManifest: string,
  output: string,
): Promise<void> {
  return new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(
      "cargo",
      [
        "run",
        "--release",
        "--manifest-path",
        cargoManifest,
        "--bin",
        "crust_plate_count_series",
      ],
      {
        cwd: process.cwd(),
        env: {
          ...process.env,
          CRUST_PLATE_SERIES_SEED: baseline.seed,
          CRUST_PLATE_SERIES_LEVEL: String(baseline.level),
          CRUST_PLATE_SERIES_TICKS: String(baseline.ticks),
          CRUST_PLATE_SERIES_RECORD_EVERY: String(baseline.record_every),
          CRUST_PLATE_SERIES_BENCH_OUT: output,
          CRUST_PLATE_SERIES_RUN_ID: "temporary-shape-gate",
        },
        stdio: "inherit",
      },
    );
    child.on("error", rejectPromise);
    child.on("exit", (code, signal) => {
      if (signal) {
        rejectPromise(new Error(`shape benchmark terminated by ${signal}`));
      } else if (code !== 0) {
        rejectPromise(new Error(`shape benchmark exited with code ${code}`));
      } else {
        resolvePromise();
      }
    });
  });
}

function aggregateMaximum(
  sample: TickRecord,
  field: keyof PlateRecord,
): number {
  const values = (sample.plates ?? [])
    .map((plate) => plate[field])
    .filter(
      (value): value is number =>
        typeof value === "number" && Number.isFinite(value),
    );
  if (values.length === 0) {
    throw new Error(`tick=${sample.tick} has no plate metric ${field}`);
  }
  return Math.max(...values);
}

function metricValue(sample: TickRecord, metric: string): number {
  if (metric === "max_component_count") {
    return aggregateMaximum(sample, "component_count");
  }
  if (metric === "max_detached_fragment_ratio") {
    return aggregateMaximum(sample, "detached_fragment_ratio");
  }
  const value = sample[metric];
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new Error(`tick=${sample.tick} has no finite metric ${metric}`);
  }
  return value;
}

function selectCurrentRecord(
  records: BenchRecord[],
  baseline: Baseline,
): BenchRecord {
  const matches = records.filter(
    (record) =>
      record.seed === baseline.seed &&
      record.level === baseline.level &&
      (record.ticks ?? 0) >= baseline.ticks &&
      Array.isArray(record.samples),
  );
  const current = matches.at(-1);
  if (!current) {
    throw new Error(
      `no ${baseline.seed} level=${baseline.level} ticks=${baseline.ticks} record`,
    );
  }
  return current;
}

function checkBaseline(baseline: Baseline, record: BenchRecord): void {
  const failures: string[] = [];
  for (const expectedSample of baseline.samples) {
    const sample = record.samples?.find(
      (candidate) => candidate.tick === expectedSample.tick,
    );
    if (!sample) {
      failures.push(`missing tick=${expectedSample.tick}`);
      continue;
    }
    for (const [metric, limit] of Object.entries(expectedSample.metrics)) {
      const value = metricValue(sample, metric);
      if (limit.min !== undefined && value < limit.min) {
        failures.push(
          `tick=${sample.tick} ${metric}=${value} < min=${limit.min}`,
        );
      }
      if (limit.max !== undefined && value > limit.max) {
        failures.push(
          `tick=${sample.tick} ${metric}=${value} > max=${limit.max}`,
        );
      }
    }
  }
  if (failures.length > 0) {
    for (const failure of failures) {
      console.error(`[temporary-plate-shape] ${failure}`);
    }
    throw new Error(
      `temporary plate shape baseline failed (${failures.length} regressions)`,
    );
  }
  console.log(
    `[temporary-plate-shape] PASS baseline=${baseline.baseline_commit} ` +
      `seed=${baseline.seed} level=${baseline.level} ticks=${baseline.ticks}`,
  );
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const baseline = await readJson<Baseline>(args.baseline);
  let currentPath = args.current;
  let temporaryDirectory: string | undefined;
  try {
    if (!currentPath) {
      temporaryDirectory = await mkdtemp(
        join(tmpdir(), "frey-plate-shape-gate-"),
      );
      currentPath = join(temporaryDirectory, "current.jsonl");
      await runCurrent(baseline, args.cargoManifest, currentPath);
    }
    const records = await readJsonLines(currentPath);
    checkBaseline(baseline, selectCurrentRecord(records, baseline));
  } finally {
    if (temporaryDirectory) {
      await rm(temporaryDirectory, { recursive: true, force: true });
    }
  }
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
