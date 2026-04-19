import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { format as formatMarkdown } from "prettier";

interface HistoryEntry {
    timestamp?: string;
    metrics?: {
        tick_total?: {
            mean?: number;
        };
    };
    memory?: {
        wasm_linear_memory_mb?: number;
    };
    runtime?: {
        wasm_init_ms?: number;
    };
}

interface PerformanceRow {
    label: string;
    value: string;
    unit: string;
    updatedAt: string;
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, "..", "..");
const TODO_PATH = path.join(REPO_ROOT, "TODO.md");
const HISTORY_PATH = path.join(REPO_ROOT, "tests/perf/history/perf-history.jsonl");
const WASM_BUNDLE_PATH = path.join(REPO_ROOT, "generated/wasm/web/frey_wasm_bg.wasm");
const START_MARKER = "<!-- performance-dashboard:start -->";
const END_MARKER = "<!-- performance-dashboard:end -->";

function toFiniteNumber(value: unknown): number | null {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
}

function formatNumber(value: number | null, digits = 3): string {
    return value == null ? "未計測" : value.toFixed(digits);
}

function formatDate(timestamp: string | undefined): string {
    if (!timestamp) {
        return "-";
    }
    const date = new Date(timestamp);
    if (Number.isNaN(date.getTime())) {
        return "-";
    }
    const parts = new Intl.DateTimeFormat("ja-JP", {
        timeZone: "Asia/Tokyo",
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
    }).formatToParts(date);
    const valueOf = (type: Intl.DateTimeFormatPartTypes) => parts.find((part) => part.type === type)?.value ?? "";
    const year = valueOf("year");
    const month = valueOf("month");
    const day = valueOf("day");
    const hour = valueOf("hour");
    const minute = valueOf("minute");
    if (!year || !month || !day || !hour || !minute) {
        return "-";
    }
    return `${year}-${month}-${day} ${hour}:${minute}`;
}

function parseHistoryLines(content: string): HistoryEntry[] {
    const entries: HistoryEntry[] = [];
    for (const line of content.split("\n")) {
        const trimmed = line.trim();
        if (trimmed.length === 0) {
            continue;
        }
        try {
            entries.push(JSON.parse(trimmed) as HistoryEntry);
        } catch {
            // Ignore malformed rows and keep scanning.
        }
    }
    return entries;
}

async function readFileSize(pathname: string): Promise<number | null> {
    try {
        const file = await readFile(pathname);
        return file.byteLength;
    } catch {
        return null;
    }
}

async function readLatestHistoryEntry(): Promise<HistoryEntry | null> {
    try {
        const content = await readFile(HISTORY_PATH, "utf8");
        const entries = parseHistoryLines(content);
        if (entries.length === 0) {
            return null;
        }

        return entries.reduce((latest, entry) => {
            const latestTime = Date.parse(latest.timestamp ?? "");
            const entryTime = Date.parse(entry.timestamp ?? "");
            if (!Number.isFinite(latestTime) && Number.isFinite(entryTime)) {
                return entry;
            }
            if (Number.isFinite(latestTime) && !Number.isFinite(entryTime)) {
                return latest;
            }
            if (!Number.isFinite(latestTime) && !Number.isFinite(entryTime)) {
                return latest;
            }
            return entryTime >= latestTime ? entry : latest;
        });
    } catch {
        return null;
    }
}

function buildRows(entry: HistoryEntry | null, bundleSizeBytes: number | null): PerformanceRow[] {
    const updatedAt = formatDate(entry?.timestamp);
    const wasmInitMs = toFiniteNumber(entry?.runtime?.wasm_init_ms);
    const tickTotalMs = toFiniteNumber(entry?.metrics?.tick_total?.mean);
    const memoryMb = toFiniteNumber(entry?.memory?.wasm_linear_memory_mb);

    return [
        {
            label: "初期読み込み時間",
            value: formatNumber(wasmInitMs),
            unit: "ms",
            updatedAt: wasmInitMs == null ? "-" : updatedAt,
        },
        {
            label: "1tick にかかる現実時間",
            value: formatNumber(tickTotalMs),
            unit: "ms/tick",
            updatedAt: tickTotalMs == null ? "-" : updatedAt,
        },
        {
            label: "WASM バンドルサイズ",
            value: formatBundleSize(bundleSizeBytes),
            unit: "KiB",
            updatedAt: bundleSizeBytes == null ? "-" : updatedAt,
        },
        {
            label: "メモリ使用量",
            value: formatNumber(memoryMb),
            unit: "MB",
            updatedAt: memoryMb == null ? "-" : updatedAt,
        },
    ];
}

function formatBundleSize(bytes: number | null): string {
    if (bytes == null) {
        return "未計測";
    }
    return `${(bytes / 1024).toFixed(2)}`;
}

function renderPerformanceBody(rows: PerformanceRow[], sourceUpdatedAt: string): string {
    const lines: string[] = [];
    lines.push("この表は `pnpm todo:perf` で更新する。");
    lines.push("");
    lines.push("| 指標 | 現在値 | 単位 | 最終更新 |");
    lines.push("| --- | ---: | --- | --- |");
    for (const row of rows) {
        lines.push(`| ${row.label} | ${row.value} | ${row.unit} | ${row.updatedAt} |`);
    }
    lines.push("");
    lines.push("- `初期読み込み時間`: `runtime.wasm_init_ms`");
    lines.push("- `1tick にかかる現実時間`: `metrics.tick_total.mean`");
    lines.push("- `WASM バンドルサイズ`: `generated/wasm/web/frey_wasm_bg.wasm`");
    lines.push("- `メモリ使用量`: `memory.wasm_linear_memory_mb`");
    lines.push(`- 出典: \`${path.relative(REPO_ROOT, HISTORY_PATH).split(path.sep).join("/")}\` の最新レコード${sourceUpdatedAt === "-" ? "" : ` (${sourceUpdatedAt})`}`);
    lines.push("");
    return lines.join("\n");
}

function replaceSection(content: string, section: string): string {
    const startIndex = content.indexOf(START_MARKER);
    const endIndex = content.indexOf(END_MARKER);
    if (startIndex === -1 || endIndex === -1 || endIndex < startIndex) {
        throw new Error(`Could not find performance dashboard markers in ${path.relative(REPO_ROOT, TODO_PATH)}`);
    }

    const before = content.slice(0, startIndex + START_MARKER.length);
    const after = content.slice(endIndex);
    const normalizedAfter = after.startsWith("\n") ? after : `\n${after}`;
    return `${before}\n${section}${normalizedAfter}`;
}

async function main() {
    const latest = await readLatestHistoryEntry();
    const bundleSizeBytes = await readFileSize(WASM_BUNDLE_PATH);
    const rows = buildRows(latest, bundleSizeBytes);
    const body = renderPerformanceBody(rows, formatDate(latest?.timestamp));
    const formattedBody = (await formatMarkdown(body, {
        parser: "markdown",
    })).trimEnd();
    const current = await readFile(TODO_PATH, "utf8");
    const updated = replaceSection(current, formattedBody);

    if (updated !== current) {
        await writeFile(TODO_PATH, updated, "utf8");
        console.log("updated TODO.md");
        return;
    }

    console.log("unchanged TODO.md");
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
