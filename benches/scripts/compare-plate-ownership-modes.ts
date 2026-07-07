import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

interface Args {
    legacyJsonl: string;
    candidateJsonl: string;
    tick?: number;
    plateId?: number;
}

interface BenchRecord {
    run_id?: string;
    seed?: string;
    samples: TickRecord[];
}

interface TickRecord {
    tick: number;
    plate_count: number;
    mean_boundary_complexity_growth?: number;
    max_boundary_complexity_growth?: number;
    persistent_boundary_complexity_growth_plate_ratio?: number;
    mean_euler_rotation_residual_ratio?: number;
    reciprocal_churn_ratio?: number;
    mean_boundary_transfer_largest_component_ratio?: number;
    max_boundary_transfer_isolated_cell_ratio?: number;
    mean_abs_plate_area_delta_ratio?: number;
    max_abs_plate_area_delta_ratio?: number;
    max_plate_area_growth_from_initial?: number;
    mean_enclosed_plate_risk?: number;
    max_enclosed_plate_risk?: number;
    mean_appendage_isolation_risk?: number;
    max_appendage_isolation_risk?: number;
    plates?: PlateRecord[];
}

interface PlateRecord {
    plate_id: number;
    cell_count?: number;
    area_ratio?: number;
    boundary_complexity_growth?: number;
    persistent_boundary_complexity_growth?: boolean;
    boundary_transfer_acquired_cell_count?: number;
    boundary_transfer_component_count?: number;
    boundary_transfer_largest_component_ratio?: number;
    boundary_transfer_isolated_cell_ratio?: number;
    euler_rotation_residual_ratio?: number;
    area_delta_ratio_per_sample?: number;
    area_growth_from_initial?: number;
    dominant_neighbor_plate_id?: number;
    dominant_neighbor_contact_ratio?: number;
    enclosed_plate_risk?: number;
    appendage_isolation_risk?: number;
}

const TICK_KEYS = [
    "mean_boundary_complexity_growth",
    "max_boundary_complexity_growth",
    "persistent_boundary_complexity_growth_plate_ratio",
    "mean_euler_rotation_residual_ratio",
    "reciprocal_churn_ratio",
    "mean_boundary_transfer_largest_component_ratio",
    "max_boundary_transfer_isolated_cell_ratio",
    "mean_abs_plate_area_delta_ratio",
    "max_abs_plate_area_delta_ratio",
    "max_plate_area_growth_from_initial",
    "mean_enclosed_plate_risk",
    "max_enclosed_plate_risk",
    "mean_appendage_isolation_risk",
    "max_appendage_isolation_risk",
] as const;

const PLATE_KEYS = [
    "cell_count",
    "area_ratio",
    "boundary_complexity_growth",
    "persistent_boundary_complexity_growth",
    "boundary_transfer_acquired_cell_count",
    "boundary_transfer_component_count",
    "boundary_transfer_largest_component_ratio",
    "boundary_transfer_isolated_cell_ratio",
    "euler_rotation_residual_ratio",
    "area_delta_ratio_per_sample",
    "area_growth_from_initial",
    "dominant_neighbor_plate_id",
    "dominant_neighbor_contact_ratio",
    "enclosed_plate_risk",
    "appendage_isolation_risk",
] as const;

function parseArgs(argv: string[]): Args {
    const args: Args = {
        legacyJsonl: "/tmp/frey-alpha-legacy-ownership.jsonl",
        candidateJsonl: "/tmp/frey-alpha-euler-front-cfl.jsonl",
        tick: 160,
        plateId: 3,
    };
    for (let i = 0; i < argv.length; i += 1) {
        const token = argv[i];
        const next = argv[i + 1];
        switch (token) {
        case "--":
            break;
        case "--legacy-jsonl":
            args.legacyJsonl = String(next ?? args.legacyJsonl);
            i += 1;
            break;
        case "--candidate-jsonl":
            args.candidateJsonl = String(next ?? args.candidateJsonl);
            i += 1;
            break;
        case "--tick":
            args.tick = Number(next);
            i += 1;
            break;
        case "--plate-id":
            args.plateId = Number(next);
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
    return args;
}

function printHelp() {
    console.error("Usage: tsx benches/scripts/compare-plate-ownership-modes.ts [options]");
    console.error("  --legacy-jsonl <path>");
    console.error("  --candidate-jsonl <path>");
    console.error("  --tick <n>");
    console.error("  --plate-id <n>");
}

async function loadLatestRecord(pathname: string): Promise<BenchRecord> {
    const content = await readFile(resolve(pathname), "utf8");
    const records = content
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => JSON.parse(line) as BenchRecord)
        .filter((record) => Array.isArray(record.samples));
    if (records.length === 0) {
        throw new Error(`No crust plate series records found in ${pathname}`);
    }
    return records[records.length - 1];
}

function pickSample(record: BenchRecord, tick?: number): TickRecord {
    if (tick !== undefined && Number.isFinite(tick)) {
        const sample = record.samples.find((candidate) => candidate.tick === tick);
        if (!sample) {
            throw new Error(`No sample tick=${tick} in run_id=${record.run_id ?? "unknown"}`);
        }
        return sample;
    }
    return record.samples[record.samples.length - 1];
}

function pickPlate(sample: TickRecord, plateId?: number): PlateRecord | null {
    if (plateId === undefined || !Number.isFinite(plateId)) {
        return null;
    }
    return sample.plates?.find((plate) => plate.plate_id === plateId) ?? null;
}

function format(value: unknown): string {
    if (typeof value === "boolean") {
        return value ? "true" : "false";
    }
    if (typeof value !== "number" || !Number.isFinite(value)) {
        return "n/a";
    }
    if (Math.abs(value) >= 100) {
        return value.toFixed(0);
    }
    return value.toFixed(6);
}

function delta(candidate: unknown, legacy: unknown): string {
    if (typeof candidate !== "number" || typeof legacy !== "number") {
        return "n/a";
    }
    if (!Number.isFinite(candidate) || !Number.isFinite(legacy)) {
        return "n/a";
    }
    const diff = candidate - legacy;
    const ratio = legacy === 0 ? "n/a" : `${(candidate / legacy).toFixed(3)}x`;
    return `${diff >= 0 ? "+" : ""}${diff.toFixed(6)} (${ratio})`;
}

function printComparison(
    title: string,
    keys: readonly string[],
    legacy: Record<string, unknown>,
    candidate: Record<string, unknown>,
) {
    console.log(`\n${title}`);
    console.log("| metric | legacy | candidate | delta |");
    console.log("| --- | ---: | ---: | ---: |");
    for (const key of keys) {
        console.log(
            `| ${key} | ${format(legacy[key])} | ${format(candidate[key])} | ${delta(candidate[key], legacy[key])} |`,
        );
    }
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const legacyRecord = await loadLatestRecord(args.legacyJsonl);
    const candidateRecord = await loadLatestRecord(args.candidateJsonl);
    const legacySample = pickSample(legacyRecord, args.tick);
    const candidateSample = pickSample(candidateRecord, args.tick);
    const legacyPlate = pickPlate(legacySample, args.plateId);
    const candidatePlate = pickPlate(candidateSample, args.plateId);

    console.log(
        `plate ownership comparison: tick=${legacySample.tick}, plate=${args.plateId ?? "n/a"}`,
    );
    console.log(`legacy: ${args.legacyJsonl} run_id=${legacyRecord.run_id ?? "unknown"}`);
    console.log(
        `candidate: ${args.candidateJsonl} run_id=${candidateRecord.run_id ?? "unknown"}`,
    );
    printComparison(
        "tick metrics",
        TICK_KEYS,
        legacySample as unknown as Record<string, unknown>,
        candidateSample as unknown as Record<string, unknown>,
    );
    if (legacyPlate && candidatePlate) {
        printComparison(
            "plate metrics",
            PLATE_KEYS,
            legacyPlate as unknown as Record<string, unknown>,
            candidatePlate as unknown as Record<string, unknown>,
        );
    }
}

main().catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
