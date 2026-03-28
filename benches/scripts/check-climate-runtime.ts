import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

interface Args {
    jsonl: string;
    baseline: string;
    threshold: number;
}

interface ClimateRecord {
    runtime?: {
        climate_step_ms?: number | null;
    };
}

function parseArgs(argv: string[]): Args {
    const args: Args = {
        jsonl: "benches/results/climate_main_scores.jsonl",
        baseline: "tests/perf/climate-bench-baseline.json",
        threshold: 0.1,
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
            args.baseline = String(next ?? args.baseline);
            i += 1;
            break;
        case "--threshold":
            args.threshold = Math.max(0, Number(next ?? args.threshold));
            i += 1;
            break;
        case "--help":
            console.error("Usage: node benches/scripts/check-climate-runtime.mjs [options]");
            console.error("  --jsonl <path>");
            console.error("  --baseline <path>");
            console.error("  --threshold <ratio>");
            process.exit(0);
            break;
        default:
            throw new Error(`Unknown argument: ${token}`);
        }
    }

    return args;
}

async function loadLatestJsonlRecord(pathname: string): Promise<ClimateRecord> {
    const content = await readFile(resolve(pathname), "utf8");
    const lines = content.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
    if (lines.length === 0) {
        throw new Error(`No records found in ${pathname}`);
    }
    return JSON.parse(lines[lines.length - 1]) as ClimateRecord;
}

async function loadJsonRecord(pathname: string): Promise<ClimateRecord> {
    const content = await readFile(resolve(pathname), "utf8");
    return JSON.parse(content) as ClimateRecord;
}

function formatRatio(value: number): string {
    return `${(value * 100).toFixed(2)}%`;
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const current = await loadLatestJsonlRecord(args.jsonl);
    const baseline = await loadJsonRecord(args.baseline);

    const currentValue = Number(current.runtime?.climate_step_ms);
    const baselineValue = Number(baseline.runtime?.climate_step_ms);
    if (!Number.isFinite(currentValue) || !Number.isFinite(baselineValue) || baselineValue <= 0) {
        throw new Error("Missing numeric climate_step_ms in current or baseline record");
    }

    const allowedMax = baselineValue * (1 + args.threshold);
    console.log(`current_climate_step_ms=${currentValue.toFixed(3)}`);
    console.log(`baseline_climate_step_ms=${baselineValue.toFixed(3)}`);
    console.log(`allowed_max_ms=${allowedMax.toFixed(3)}`);

    if (currentValue > allowedMax) {
        console.error(
            `[climate runtime gate] FAIL current=${currentValue.toFixed(3)} baseline=${baselineValue.toFixed(3)} threshold=${formatRatio(args.threshold)} allowed_max=${allowedMax.toFixed(3)}`,
        );
        process.exit(1);
    }

    console.log(`[climate runtime gate] PASS threshold=${formatRatio(args.threshold)}`);
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
