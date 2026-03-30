import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import readline from "node:readline";

import blessed from "blessed";
import contrib from "blessed-contrib";

const UI_COLOR = "cyan";

const BENCHES = [
    {
        id: "climate_solo",
        title: "CLIMATE_SOLO",
        jsonlPath: "benches/results/climate_main_scores.jsonl",
        command: ["cargo", "bench", "--manifest-path", "rust/Cargo.toml", "--bench", "climate_solo"],
        summarize(record: Record<string, unknown>) {
            const phase2 = (record as { phase2?: { metrics?: Record<string, unknown> } })?.phase2?.metrics ?? {};
            const phase1 = (record as { phase1?: Record<string, { matched?: unknown }> })?.phase1 ?? {};
            const rankingMatched = ["temperature", "precipitation", "aridity"]
                .map((key) => Number(phase1?.[key]?.matched))
                .filter((value) => Number.isFinite(value))
                .reduce((sum, value) => sum + (value > 0 ? 1 : 0), 0);
            return [
                ["temperature_rho", (phase2 as Record<string, unknown>).temperature],
                ["precipitation_rho", (phase2 as Record<string, unknown>).precipitation],
                ["evapotrans_rho", (phase2 as Record<string, unknown>).evapotranspiration],
                ["runoff_rho", (phase2 as Record<string, unknown>).runoff],
                ["aridity_rho", (phase2 as Record<string, unknown>).aridity],
                [
                    "ranking_assertions",
                    {
                        matched: rankingMatched,
                        total: 3,
                        ratio: 3 > 0 ? rankingMatched / 3 : null,
                    },
                ],
            ] as [string, unknown][];
        },
    },
    {
        id: "hydrology_solo",
        title: "HYDROLOGY_SOLO",
        jsonlPath: "benches/results/hydrology_main_scores.jsonl",
        command: ["cargo", "bench", "--manifest-path", "rust/Cargo.toml", "--bench", "hydrology_solo"],
        summarize(record: Record<string, unknown>) {
            const main = (record as { main_evaluation?: { metrics?: Record<string, unknown> } })?.main_evaluation?.metrics ?? {};
            const ranking = (record as { diagnostic_evaluation?: { river_flow_assertions?: Record<string, unknown> } })?.diagnostic_evaluation?.river_flow_assertions ?? {};
            return [
                ["river_flow_rho", (main as Record<string, unknown>).river_flow_rho],
                ["is_lake_f1", (main as Record<string, unknown>).is_lake_f1],
                [
                    "ranking_assertions",
                    {
                        matched: (ranking as Record<string, unknown>).matched,
                        total: (ranking as Record<string, unknown>).total,
                        ratio: (ranking as Record<string, unknown>).coverage_ratio,
                    },
                ],
            ] as [string, unknown][];
        },
    },
    {
        id: "ecology_solo",
        title: "ECOLOGY_SOLO",
        jsonlPath: "benches/results/ecology_main_scores.jsonl",
        command: ["cargo", "bench", "--manifest-path", "rust/Cargo.toml", "--bench", "ecology_solo"],
        summarize(record: Record<string, unknown>) {
            const main = (record as { main_evaluation?: { metrics?: Record<string, unknown> } })?.main_evaluation?.metrics ?? {};
            const ranking = (record as { diagnostic_evaluation?: { biome_assertions?: Record<string, unknown> } })?.diagnostic_evaluation?.biome_assertions ?? {};
            return [
                ["tree_cover_rho", (main as Record<string, unknown>).tree_cover_rho],
                ["ground_cover_rho", (main as Record<string, unknown>).ground_cover_rho],
                ["biome_macro_f1", (main as Record<string, unknown>).biome_macro_f1],
                [
                    "ranking_assertions",
                    {
                        matched: (ranking as Record<string, unknown>).matched,
                        total: (ranking as Record<string, unknown>).total,
                        ratio: (ranking as Record<string, unknown>).coverage_ratio,
                    },
                ],
            ] as [string, unknown][];
        },
    },
];

function toFiniteNumber(value: unknown) {
    const num = Number(value);
    return Number.isFinite(num) ? num : null;
}

function formatNumber(value: unknown, digits = 3) {
    const num = toFiniteNumber(value);
    return num == null ? "-" : num.toFixed(digits);
}

function formatDelta(current: unknown, previous: unknown) {
    const c = toFiniteNumber(current);
    const p = toFiniteNumber(previous);
    if (c == null || p == null) {
        return "-";
    }
    const diff = c - p;
    const sign = diff >= 0 ? "+" : "";
    return `${sign}${diff.toFixed(3)}`;
}

function parseRankingValue(value: unknown) {
    if (!value || typeof value !== "object") {
        return null;
    }
    const obj = value as Record<string, unknown>;
    const matched = toFiniteNumber(obj.matched);
    const total = toFiniteNumber(obj.total);
    const ratio = toFiniteNumber(obj.ratio);
    if (matched == null || total == null || total <= 0) {
        return null;
    }
    return {
        matched,
        total,
        ratio: ratio == null ? matched / total : ratio,
    };
}

function formatMetricValue(value: unknown) {
    const ranking = parseRankingValue(value);
    if (ranking) {
        return `${Math.round(ranking.matched)}/${Math.round(ranking.total)} (${ranking.ratio.toFixed(3)})`;
    }
    return formatNumber(value);
}

function metricToChartValue(value: unknown) {
    const ranking = parseRankingValue(value);
    if (ranking) {
        return ranking.ratio;
    }
    const num = toFiniteNumber(value);
    if (num == null) {
        return null;
    }
    return Math.max(0, Math.min(1, num));
}

function readParsedJsonLines(content: string) {
    const rows: Record<string, unknown>[] = [];
    for (const line of content.split("\n")) {
        const trimmed = line.trim();
        if (trimmed.length === 0) {
            continue;
        }
        try {
            rows.push(JSON.parse(trimmed));
        } catch {
            // Ignore malformed rows and keep scanning.
        }
    }
    return rows;
}

async function readRecentRecords(pathname: string, count = 2) {
    try {
        const content = await readFile(resolve(pathname), "utf8");
        const parsed = readParsedJsonLines(content);
        if (parsed.length === 0) {
            return [];
        }
        return parsed.slice(Math.max(0, parsed.length - count));
    } catch {
        return [];
    }
}

interface BenchState {
    status?: string;
    startedAt?: number | null;
    endedAt?: number | null;
    elapsedMs?: number | null;
    summary?: [string, unknown][];
    previousSummary?: [string, unknown][];
}

function renderStatusTable(widget: { setData: (data: { headers: string[]; data: string[][] }) => void }, states: Record<string, BenchState>) {
    const rows = BENCHES.map((bench, index) => {
        const state = states[bench.id] ?? {};
        const started = state.startedAt ? new Date(state.startedAt).toLocaleTimeString() : "-";
        const ended = state.endedAt ? new Date(state.endedAt).toLocaleTimeString() : "-";
        const elapsed = state.elapsedMs != null ? `${(state.elapsedMs / 1000).toFixed(1)}s` : "-";
        return [String(index + 1), bench.title, state.status ?? "pending", elapsed, started, ended];
    });
    widget.setData({
        headers: ["#", "module", "status", "elapsed", "start", "end"],
        data: rows,
    });
}

function renderMetricsTable(widget: { setData: (data: { headers: string[]; data: string[][] }) => void }, activeBenchId: string | null, states: Record<string, BenchState>) {
    const state = activeBenchId ? states[activeBenchId] : null;
    const current = state?.summary ?? [];
    const previousMap = new Map((state?.previousSummary ?? []).map(([name, value]) => [name, value]));
    const rows = current.map(([name, value]) => {
        const prev = previousMap.get(name);
        if (name === "ranking_assertions") {
            const currRanking = parseRankingValue(value);
            const prevRanking = parseRankingValue(prev);
            const currRatio = currRanking?.ratio ?? null;
            const prevRatio = prevRanking?.ratio ?? null;
            return [name, formatMetricValue(value), formatMetricValue(prev), formatDelta(currRatio, prevRatio)];
        }
        return [name, formatMetricValue(value), formatMetricValue(prev), formatDelta(value, prev)];
    });
    widget.setData({
        headers: ["metric", "current", "prev", "delta"],
        data: rows.length > 0 ? rows : [["-", "-", "-", "-"]],
    });
}

function renderChart(widget: { setData: (data: unknown[]) => void }, activeBenchId: string | null, states: Record<string, BenchState>) {
    const state = activeBenchId ? states[activeBenchId] : null;
    const current = state?.summary ?? [];
    const previousMap = new Map((state?.previousSummary ?? []).map(([name, value]) => [name, value]));

    const labels = [];
    const currentValues = [];
    const previousValues = [];
    for (const [name, value] of current) {
        const curr = metricToChartValue(value);
        const prev = metricToChartValue(previousMap.get(name));
        if (curr == null && prev == null) {
            continue;
        }
        labels.push(name.replace("_", "\n"));
        currentValues.push(curr ?? 0);
        previousValues.push(prev ?? 0);
    }

    if (labels.length === 0) {
        widget.setData([]);
        return;
    }

    widget.setData([
        {
            title: "current",
            x: labels,
            y: currentValues,
            style: {
                line: "green",
            },
        },
        {
            title: "previous",
            x: labels,
            y: previousValues,
            style: {
                line: "yellow",
            },
        },
    ]);
}

function pushSummaryLog(logWidget: { log: (message: string) => void }, title: string, summary: [string, unknown][], previousSummary: [string, unknown][]) {
    const previousMap = new Map(previousSummary.map(([name, value]) => [name, value]));
    logWidget.log(`[${title}]`);
    for (const [name, value] of summary) {
        const padName = name.padEnd(18, " ");
        if (name === "ranking_assertions") {
            const curr = parseRankingValue(value);
            const prev = parseRankingValue(previousMap.get(name));
            const delta = formatDelta(curr?.ratio, prev?.ratio);
            logWidget.log(`  - ${padName}: ${formatMetricValue(value)}  (Δ ${delta})`);
            continue;
        }
        const delta = formatDelta(value, previousMap.get(name));
        logWidget.log(`  - ${padName}: ${formatMetricValue(value)}  (Δ ${delta})`);
    }
    logWidget.log("");
}

function runBench(bench: typeof BENCHES[number], states: Record<string, BenchState>, onLog: (message: string) => void, onUpdate: (id: string) => void) {
    return new Promise((resolveRun) => {
        const startedAt = Date.now();
        const currentState = states[bench.id] ?? {};
        states[bench.id] = {
            ...currentState,
            status: "running",
            startedAt,
            endedAt: null,
            elapsedMs: null,
        };
        onUpdate(bench.id);

        const child = spawn(bench.command[0], bench.command.slice(1), {
            cwd: resolve("."),
            stdio: ["ignore", "pipe", "pipe"],
            env: process.env,
        });

        const stdoutReader = readline.createInterface({ input: child.stdout });
        stdoutReader.on("line", (line) => {
            const trimmed = line.trim();
            if (trimmed.length > 0) {
                onLog(`[${bench.title}][OUT] ${trimmed}`);
            }
        });

        const stderrReader = readline.createInterface({ input: child.stderr });
        stderrReader.on("line", (line) => {
            const trimmed = line.trim();
            if (trimmed.length > 0) {
                onLog(`[${bench.title}][ERR] ${trimmed}`);
            }
        });

        child.on("error", (error) => {
            const endedAt = Date.now();
            states[bench.id] = {
                ...states[bench.id],
                status: "error",
                endedAt,
                elapsedMs: endedAt - startedAt,
            };
            onLog(`[${bench.title}] process error: ${error.message}`);
            onUpdate(bench.id);
            resolveRun(undefined);
        });

        child.on("close", async (code, signal) => {
            const endedAt = Date.now();
            const status = code === 0 ? "done" : `error(${code ?? "?"}${signal ? `/${signal}` : ""})`;
            const next: BenchState = {
                ...(states[bench.id] ?? {}),
                status,
                endedAt,
                elapsedMs: endedAt - startedAt,
            };

            const recent = await readRecentRecords(bench.jsonlPath, 2);
            if (recent.length > 0) {
                next.summary = bench.summarize(recent[recent.length - 1]);
                next.previousSummary = recent.length > 1 ? bench.summarize(recent[recent.length - 2]) : [];
            }

            states[bench.id] = next;
            onUpdate(bench.id);
            resolveRun(undefined);
        });
    });
}

async function preloadHistory(states: Record<string, BenchState>) {
    for (const bench of BENCHES) {
        const recent = await readRecentRecords(bench.jsonlPath, 2);
        states[bench.id] = {
            status: "pending",
            startedAt: null,
            endedAt: null,
            elapsedMs: null,
            summary: recent.length > 0 ? bench.summarize(recent[recent.length - 1]) : [],
            previousSummary: recent.length > 1 ? bench.summarize(recent[recent.length - 2]) : [],
        };
    }
}

async function main() {
    const screen = blessed.screen({
        smartCSR: true,
        title: "Frey Bench Dashboard",
        fullUnicode: true,
    });

    const grid = new contrib.grid({ rows: 12, cols: 12, screen });
    const progress = grid.set(0, 0, 1, 12, blessed.progressbar, {
        label: " Progress ",
        border: "line",
        orientation: "horizontal",
        pch: " ",
        filled: 0,
        style: {
            border: { fg: UI_COLOR },
            bar: { bg: UI_COLOR, fg: UI_COLOR },
            bg: "black",
        },
    });

    const statusTable = grid.set(1, 0, 4, 7, contrib.table, {
        label: " Module Status ",
        keys: false,
        interactive: false,
        columnSpacing: 1,
        columnWidth: [3, 16, 14, 10, 10, 10],
        border: { fg: UI_COLOR },
    });

    const metricsTable = grid.set(1, 7, 4, 5, contrib.table, {
        label: " Metrics (Current vs Previous) ",
        keys: false,
        interactive: false,
        columnSpacing: 1,
        columnWidth: [20, 16, 16, 10],
        border: { fg: UI_COLOR },
    });

    const chart = grid.set(5, 0, 3, 12, contrib.line, {
        label: " Visualized Metrics (0..1) ",
        showLegend: true,
        wholeNumbersOnly: false,
        minY: 0,
        maxY: 1,
        border: { fg: UI_COLOR },
    });

    const log = grid.set(8, 0, 4, 12, contrib.log, {
        label: " Log ",
        fg: "white",
        selectedFg: "white",
        border: { fg: UI_COLOR },
    });

    const states: Record<string, BenchState> = {};
    let activeBenchId: string | null = null;

    await preloadHistory(states);

    function rerender(currentBenchId: string | null = activeBenchId) {
        if (currentBenchId) {
            activeBenchId = currentBenchId;
        }
        renderStatusTable(statusTable, states);
        renderMetricsTable(metricsTable, activeBenchId, states);
        renderChart(chart, activeBenchId, states);
        const doneCount = BENCHES.filter((bench) => {
            const status = states[bench.id]?.status ?? "pending";
            return status.startsWith("done") || status.startsWith("error");
        }).length;
        const ratio = doneCount / BENCHES.length;
        progress.setProgress(Math.round(ratio * 100));
        screen.render();
    }

    function pushLog(message: string) {
        log.log(message);
        screen.render();
    }

    screen.key(["q", "C-c"], () => {
        screen.destroy();
        process.exit(0);
    });

    pushLog("bench dashboard started");
    rerender(BENCHES[0].id);

    for (const bench of BENCHES) {
        activeBenchId = bench.id;
        rerender(bench.id);
        pushLog(`[${bench.title}] start`);
        await runBench(bench, states, pushLog, rerender);
        const doneState = states[bench.id] ?? {};
        pushLog(`[${bench.title}] finish status=${doneState.status ?? "unknown"}`);
        if (Array.isArray(doneState.summary) && doneState.summary.length > 0) {
            pushSummaryLog(log, bench.title, doneState.summary, doneState.previousSummary ?? []);
        }
    }

    pushLog("all benchmark jobs completed (press q to quit)");
    rerender(activeBenchId);
}

main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
});
