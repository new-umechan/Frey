import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

interface Args {
    jsonl: string;
    baseline: string | null;
    writeBaseline: string | null;
}

interface DomesticatesScoreRecord {
    timestamp_unix_ms: number;
    runtime?: {
        domesticates_step_ms?: number | null;
    };
    metrics?: Record<string, number | null>;
}

const METRIC_KEYS = [
    "crop_intensity_rho",
    "crop_presence_f1",
    "livestock_intensity_rho",
    "livestock_presence_f1",
    "regional_assertion_coverage",
    "overall_score",
] as const;

function parseArgs(argv: string[]): Args {
    const args: Args = {
        jsonl: "benches/results/domesticates_main_scores.jsonl",
        baseline: null,
        writeBaseline: null,
    };

    for (let i = 0; i < argv.length; i += 1) {
        const token = argv[i];
        const next = argv[i + 1];
        switch (token) {
        case "--":
            break;
        case "--jsonl":
            args.jsonl = String(next ?? args.jsonl);
            i += 1;
            break;
        case "--baseline":
            args.baseline = String(next ?? "");
            i += 1;
            break;
        case "--write-baseline":
            args.writeBaseline = String(next ?? "");
            i += 1;
            break;
        case "--help":
            console.error("Usage: tsx benches/scripts/compare-domesticates-scores.ts [options]");
            console.error("  --jsonl <path>");
            console.error("  --baseline <path>");
            console.error("  --write-baseline <path>");
            process.exit(0);
            break;
        default:
            throw new Error(`Unknown argument: ${token}`);
        }
    }

    return args;
}

async function loadJsonlRecords(pathname: string): Promise<DomesticatesScoreRecord[]> {
    const content = await readFile(resolve(pathname), "utf8");
    return content
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => JSON.parse(line) as DomesticatesScoreRecord);
}

async function loadJson(pathname: string): Promise<DomesticatesScoreRecord> {
    const content = await readFile(resolve(pathname), "utf8");
    return JSON.parse(content) as DomesticatesScoreRecord;
}

async function saveJson(pathname: string, record: DomesticatesScoreRecord): Promise<void> {
    const outputPath = resolve(pathname);
    await mkdir(dirname(outputPath), { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(record, null, 2)}\n`, "utf8");
}

function formatValue(value: number | null | undefined, digits = 3): string {
    if (!Number.isFinite(Number(value))) {
        return "n/a";
    }
    return Number(value).toFixed(digits);
}

function formatDelta(current: number | null | undefined, baseline: number | null | undefined, digits = 3): string {
    if (!Number.isFinite(Number(current)) || !Number.isFinite(Number(baseline))) {
        return "n/a";
    }
    const delta = Number(current) - Number(baseline);
    return `${delta >= 0 ? "+" : ""}${delta.toFixed(digits)}`;
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const records = await loadJsonlRecords(args.jsonl);
    if (records.length === 0) {
        throw new Error(`No records found in ${args.jsonl}`);
    }

    const current = records[records.length - 1];
    if (args.writeBaseline) {
        await saveJson(args.writeBaseline, current);
        console.log(`Baseline saved: ${resolve(args.writeBaseline)}`);
    }

    const baseline = args.baseline
        ? await loadJson(args.baseline)
        : records.length >= 2
            ? records[records.length - 2]
            : null;

    if (!baseline) {
        console.log("Only one record is available. Baseline comparison skipped.");
        return;
    }

    console.log("=== Domesticates Score Comparison ===");
    console.log(`current_timestamp_unix_ms=${current.timestamp_unix_ms}`);
    console.log(`baseline_timestamp_unix_ms=${baseline.timestamp_unix_ms}`);
    console.log("");
    console.log(
        `runtime_ms: current=${formatValue(current.runtime?.domesticates_step_ms)} baseline=${formatValue(baseline.runtime?.domesticates_step_ms)} delta=${formatDelta(current.runtime?.domesticates_step_ms, baseline.runtime?.domesticates_step_ms)}`,
    );
    console.log("");

    for (const key of METRIC_KEYS) {
        console.log(
            `${key}: current=${formatValue(current.metrics?.[key])} baseline=${formatValue(baseline.metrics?.[key])} delta=${formatDelta(current.metrics?.[key], baseline.metrics?.[key])}`,
        );
    }
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
