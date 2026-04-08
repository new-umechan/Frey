import { readFile, readdir } from "node:fs/promises";
import { resolve } from "node:path";

interface HistoryEntry {
    timestamp?: string;
    commit?: string;
    branch?: string;
    metrics?: Record<string, { mean?: number }>;
    memory?: {
        wasm_linear_memory_mb?: number;
    };
}

interface CliArgs {
    limit: number;
}

function parseArgs(argv: string[]): CliArgs {
    const args: CliArgs = {
        limit: 10,
    };
    for (let i = 0; i < argv.length; i += 1) {
        const token = argv[i];
        const next = argv[i + 1];
        if (token === "--") {
            continue;
        }
        switch (token) {
        case "--limit":
            args.limit = Math.max(1, Math.floor(Number(next)));
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
    console.error("Usage: pnpm perf:history -- --limit <n>");
}

function toNumber(value: unknown): number | null {
    const n = Number(value);
    return Number.isFinite(n) ? n : null;
}

function pickMean(entry: HistoryEntry, metricName: string): number | null {
    return toNumber(entry.metrics?.[metricName]?.mean);
}

function formatMetric(value: number | null): string {
    if (value == null) {
        return "-";
    }
    return value.toFixed(3);
}

function parseJsonLine(line: string): HistoryEntry | null {
    const trimmed = line.trim();
    if (trimmed.length === 0) {
        return null;
    }
    try {
        const parsed = JSON.parse(trimmed) as HistoryEntry;
        return parsed;
    } catch {
        return null;
    }
}

async function readHistoryEntries(historyDir: string): Promise<HistoryEntry[]> {
    let files: string[] = [];
    try {
        files = await readdir(historyDir);
    } catch {
        return [];
    }
    const jsonlFiles = files
        .filter((name) => name.endsWith(".jsonl"))
        .sort();
    const entries: HistoryEntry[] = [];
    for (const filename of jsonlFiles) {
        const content = await readFile(resolve(historyDir, filename), "utf8");
        const lines = content.split("\n");
        for (const line of lines) {
            const parsed = parseJsonLine(line);
            if (parsed) {
                entries.push(parsed);
            }
        }
    }
    entries.sort((a, b) => {
        const at = Date.parse(a.timestamp ?? "");
        const bt = Date.parse(b.timestamp ?? "");
        if (!Number.isFinite(at) && !Number.isFinite(bt)) {
            return 0;
        }
        if (!Number.isFinite(at)) {
            return 1;
        }
        if (!Number.isFinite(bt)) {
            return -1;
        }
        return bt - at;
    });
    return entries;
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const historyDir = resolve("tests/perf/history");
    const entries = await readHistoryEntries(historyDir);

    if (entries.length === 0) {
        console.log("No history entries.");
        return;
    }

    const picked = entries.slice(0, args.limit);
    console.log("timestamp\tcommit\tbranch\ttick_total.mean\texec_world.mean\tstep_geology_river.mean\twasm_linear_memory_mb");
    for (const entry of picked) {
        const tickTotal = pickMean(entry, "tick_total");
        const execWorld = pickMean(entry, "exec_world");
        const river = pickMean(entry, "step_geology_river");
        const memoryMb = toNumber(entry.memory?.wasm_linear_memory_mb);
        console.log([
            entry.timestamp ?? "-",
            entry.commit ?? "-",
            entry.branch ?? "-",
            formatMetric(tickTotal),
            formatMetric(execWorld),
            formatMetric(river),
            formatMetric(memoryMb),
        ].join("\t"));
    }
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
