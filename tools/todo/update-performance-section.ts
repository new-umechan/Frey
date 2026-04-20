import { readFile, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

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

interface PerformanceValue {
  label: string;
  value: string;
  unit: string;
  updatedAt: string;
}

interface PerformanceRow extends PerformanceValue {
  previousValue: string;
  delta: string;
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, "..", "..");
const TODO_PATH = path.join(REPO_ROOT, "TODO.md");
const HISTORY_PATH = path.join(
  REPO_ROOT,
  "tests/perf/history/perf-history.jsonl",
);
const SNAPSHOT_HISTORY_PATH = path.join(
  REPO_ROOT,
  "tests/perf/history/perf-dashboard-snapshot-history.json",
);
const WASM_BUNDLE_PATH = path.join(
  REPO_ROOT,
  "generated/wasm/web/frey_wasm_bg.wasm",
);
const START_MARKER = "<!-- performance-dashboard:start -->";
const END_MARKER = "<!-- performance-dashboard:end -->";
const MAX_SNAPSHOT_HISTORY = 32;

function toRepoRelativePath(pathname: string): string {
  return path.relative(REPO_ROOT, pathname).split(path.sep).join("/");
}

function isEnoentError(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error != null &&
    "code" in error &&
    (error as { code?: string }).code === "ENOENT"
  );
}

function toFiniteNumber(value: unknown): number | null {
  if (value == null || value === "") {
    return null;
  }
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
  const valueOf = (type: Intl.DateTimeFormatPartTypes) =>
    parts.find((part) => part.type === type)?.value ?? "";
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
  const lines = content.split("\n");
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index] ?? "";
    const trimmed = line.trim();
    if (trimmed.length === 0) {
      continue;
    }
    try {
      entries.push(JSON.parse(trimmed) as HistoryEntry);
    } catch (error) {
      throw new Error(
        `Malformed JSONL row at ${toRepoRelativePath(HISTORY_PATH)}:${index + 1}: ${String(error)}`,
      );
    }
  }
  return entries;
}

async function readFileSize(pathname: string): Promise<number | null> {
  try {
    const fileStat = await stat(pathname);
    return fileStat.size;
  } catch (error) {
    if (isEnoentError(error)) {
      return null;
    }
    throw error;
  }
}

async function readLatestHistoryEntry(): Promise<HistoryEntry | null> {
  let content: string;
  try {
    content = await readFile(HISTORY_PATH, "utf8");
  } catch (error) {
    if (isEnoentError(error)) {
      return null;
    }
    throw error;
  }

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
}

function formatBundleSize(bytes: number | null): string {
  if (bytes == null) {
    return "未計測";
  }
  return `${(bytes / 1024).toFixed(2)}`;
}

export function extractPerformanceSection(content: string): string | null {
  const startIndex = content.indexOf(START_MARKER);
  const endIndex = content.indexOf(END_MARKER);
  if (startIndex === -1 || endIndex === -1 || endIndex < startIndex) {
    return null;
  }

  return content.slice(startIndex + START_MARKER.length, endIndex);
}

export function parsePerformanceValues(section: string): PerformanceValue[] {
  const rows: PerformanceValue[] = [];
  let header: {
    label: number;
    value: number;
    unit: number;
    updatedAt: number;
  } | null = null;

  const parseCells = (rawLine: string): string[] | null => {
    const line = rawLine.trim();
    if (!line.startsWith("|") || !line.endsWith("|")) {
      return null;
    }
    return line
      .slice(1, -1)
      .split("|")
      .map((cell) => cell.trim());
  };

  const isDividerRow = (cells: string[]): boolean =>
    cells.length > 0 &&
    cells.every((cell) => /^:?-{3,}:?$/.test(cell.replace(/\s/g, "")));

  for (const rawLine of section.split("\n")) {
    const cells = parseCells(rawLine);
    if (cells == null) {
      if (header != null && rows.length > 0) {
        break;
      }
      continue;
    }

    if (header == null) {
      const labelIndex = cells.indexOf("指標");
      const valueIndex = cells.indexOf("現在値");
      const unitIndex = cells.indexOf("単位");
      const updatedAtIndex = cells.indexOf("最終更新");
      if (
        labelIndex >= 0 &&
        valueIndex >= 0 &&
        unitIndex >= 0 &&
        updatedAtIndex >= 0
      ) {
        header = {
          label: labelIndex,
          value: valueIndex,
          unit: unitIndex,
          updatedAt: updatedAtIndex,
        };
      }
      continue;
    }

    if (isDividerRow(cells)) {
      continue;
    }

    const maxIndex = Math.max(
      header.label,
      header.value,
      header.unit,
      header.updatedAt,
    );
    if (cells.length <= maxIndex) {
      continue;
    }

    rows.push({
      label: cells[header.label] ?? "",
      value: cells[header.value] ?? "",
      unit: cells[header.unit] ?? "",
      updatedAt: cells[header.updatedAt] ?? "",
    });
  }

  return rows;
}

export function arePerformanceValuesEqual(
  left: PerformanceValue[],
  right: PerformanceValue[],
): boolean {
  if (left.length !== right.length) {
    return false;
  }

  return left.every(
    (row, index) =>
      row.label === right[index]?.label &&
      row.value === right[index]?.value &&
      row.unit === right[index]?.unit,
  );
}

function buildCurrentValues(
  entry: HistoryEntry | null,
  bundleSizeBytes: number | null,
): PerformanceValue[] {
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

export function buildComparisonRowsWithFallback(
  currentValues: PerformanceValue[],
  previousValues: PerformanceValue[],
  snapshotHistory: PerformanceValue[][],
): PerformanceRow[] {
  const orderedHistory = [previousValues, ...snapshotHistory];
  return currentValues.map((current) => {
    let comparisonBase: PerformanceValue | null = null;
    for (const snapshot of orderedHistory) {
      const candidate = snapshot.find((row) => row.label === current.label);
      if (candidate == null) {
        continue;
      }
      if (comparisonBase == null) {
        comparisonBase = candidate;
      }
      if (!areMetricValuesEqual(current.value, candidate.value)) {
        comparisonBase = candidate;
        break;
      }
    }

    const currentNumber = toFiniteNumber(current.value);
    const previousNumber = toFiniteNumber(comparisonBase?.value ?? null);
    return {
      ...current,
      previousValue: comparisonBase?.value ?? "未計測",
      delta: formatDelta(
        currentNumber,
        previousNumber,
        current.label === "WASM バンドルサイズ" ? 2 : 3,
      ),
    };
  });
}

function areMetricValuesEqual(current: string, previous: string): boolean {
  const currentNumber = toFiniteNumber(current);
  const previousNumber = toFiniteNumber(previous);
  if (currentNumber != null && previousNumber != null) {
    return currentNumber === previousNumber;
  }
  return current === previous;
}

function formatDelta(
  current: number | null,
  previous: number | null,
  digits = 3,
): string {
  if (current == null || previous == null) {
    return "-";
  }
  const delta = current - previous;
  return `${delta >= 0 ? "+" : ""}${delta.toFixed(digits)}`;
}

function renderPerformanceBody(rows: PerformanceRow[]): string {
  const lines: string[] = [];
  lines.push("この表は `pnpm todo:perf` で更新する。");
  lines.push("");
  lines.push("差分が 0 のときは、履歴をさかのぼって最初に差分が出る値を使う。");
  lines.push("");
  lines.push("| 指標 | 前回値 | 現在値 | 差分 | 単位 | 最終更新 |");
  lines.push("| --- | ---: | ---: | ---: | --- | --- |");
  for (const row of rows) {
    lines.push(
      `| ${row.label} | ${row.previousValue} | ${row.value} | ${row.delta} | ${row.unit} | ${row.updatedAt} |`,
    );
  }
  lines.push("");
  return lines.join("\n");
}

function parsePerformanceValue(
  value: unknown,
  source: string,
): PerformanceValue {
  if (value == null || typeof value !== "object") {
    throw new Error(`Invalid snapshot value in ${source}: object expected`);
  }
  const row = value as Record<string, unknown>;
  if (
    typeof row.label !== "string" ||
    typeof row.value !== "string" ||
    typeof row.unit !== "string" ||
    typeof row.updatedAt !== "string"
  ) {
    throw new Error(
      `Invalid snapshot value in ${source}: {label,value,unit,updatedAt} string fields are required`,
    );
  }
  return {
    label: row.label,
    value: row.value,
    unit: row.unit,
    updatedAt: row.updatedAt,
  };
}

function parseSnapshotHistoryPayload(
  value: unknown,
  source: string,
): PerformanceValue[][] {
  if (!Array.isArray(value)) {
    throw new Error(`Invalid snapshot history in ${source}: array expected`);
  }

  return value
    .map((snapshot, snapshotIndex) => {
      if (!Array.isArray(snapshot)) {
        throw new Error(
          `Invalid snapshot history in ${source}[${snapshotIndex}]: array expected`,
        );
      }
      const values = snapshot.map((item, itemIndex) =>
        parsePerformanceValue(
          item,
          `${source}[${snapshotIndex}][${itemIndex}]`,
        ),
      );
      return values.length > 0 ? values : null;
    })
    .filter((snapshot): snapshot is PerformanceValue[] => snapshot != null)
    .slice(0, MAX_SNAPSHOT_HISTORY);
}

function areSnapshotHistoriesEqual(
  left: PerformanceValue[][],
  right: PerformanceValue[][],
): boolean {
  if (left.length !== right.length) {
    return false;
  }
  return left.every((snapshot, index) =>
    arePerformanceValuesEqual(snapshot, right[index] ?? []),
  );
}

async function readPersistedSnapshotHistory(): Promise<
  PerformanceValue[][] | null
> {
  let content: string;
  try {
    content = await readFile(SNAPSHOT_HISTORY_PATH, "utf8");
  } catch (error) {
    if (isEnoentError(error)) {
      return null;
    }
    throw error;
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(content);
  } catch (error) {
    throw new Error(
      `Malformed JSON in ${toRepoRelativePath(SNAPSHOT_HISTORY_PATH)}: ${String(error)}`,
    );
  }

  if (parsed == null || typeof parsed !== "object") {
    throw new Error(
      `Invalid snapshot store in ${toRepoRelativePath(SNAPSHOT_HISTORY_PATH)}: object expected`,
    );
  }

  const store = parsed as { version?: unknown; snapshots?: unknown };
  if (store.version !== 1) {
    throw new Error(
      `Invalid snapshot store in ${toRepoRelativePath(SNAPSHOT_HISTORY_PATH)}: unsupported version`,
    );
  }

  return parseSnapshotHistoryPayload(
    store.snapshots,
    `${toRepoRelativePath(SNAPSHOT_HISTORY_PATH)}.snapshots`,
  );
}

async function writePersistedSnapshotHistory(
  snapshotHistory: PerformanceValue[][],
): Promise<void> {
  const payload = {
    version: 1,
    snapshots: snapshotHistory.slice(0, MAX_SNAPSHOT_HISTORY),
  };
  await writeFile(
    SNAPSHOT_HISTORY_PATH,
    `${JSON.stringify(payload, null, 2)}\n`,
    "utf8",
  );
}

function buildNextSnapshotHistory(
  currentValues: PerformanceValue[],
  snapshotHistory: PerformanceValue[][],
): PerformanceValue[][] {
  const combined = [currentValues, ...snapshotHistory];
  const deduped: PerformanceValue[][] = [];
  for (const snapshot of combined) {
    if (snapshot.length === 0) {
      continue;
    }
    const last = deduped[deduped.length - 1];
    if (last != null && arePerformanceValuesEqual(last, snapshot)) {
      continue;
    }
    deduped.push(snapshot);
  }
  return deduped.slice(0, MAX_SNAPSHOT_HISTORY);
}

function replaceSection(content: string, section: string): string {
  const startIndex = content.indexOf(START_MARKER);
  const endIndex = content.indexOf(END_MARKER);
  if (startIndex === -1 || endIndex === -1 || endIndex < startIndex) {
    throw new Error(
      `Could not find performance dashboard markers in ${path.relative(REPO_ROOT, TODO_PATH)}`,
    );
  }

  const before = content.slice(0, startIndex + START_MARKER.length);
  const after = content.slice(endIndex);
  const normalizedAfter = after.startsWith("\n") ? after : `\n${after}`;
  return `${before}\n${section}${normalizedAfter}`;
}

async function main() {
  const latest = await readLatestHistoryEntry();
  const bundleSizeBytes = await readFileSize(WASM_BUNDLE_PATH);
  const current = await readFile(TODO_PATH, "utf8");
  const persistedSnapshotHistory = await readPersistedSnapshotHistory();
  const snapshotHistory = persistedSnapshotHistory ?? [];
  const currentValues = buildCurrentValues(latest, bundleSizeBytes);
  const previousValues = snapshotHistory[0] ?? [];
  const rows = buildComparisonRowsWithFallback(
    currentValues,
    previousValues,
    snapshotHistory.slice(1),
  );
  const nextSnapshotHistory = buildNextSnapshotHistory(
    currentValues,
    snapshotHistory,
  );

  const body = renderPerformanceBody(rows);
  const formattedBody = (
    await formatMarkdown(body, {
      parser: "markdown",
    })
  ).trimEnd();
  const updated = replaceSection(current, formattedBody);

  const updatedTargets: string[] = [];
  if (updated !== current) {
    await writeFile(TODO_PATH, updated, "utf8");
    updatedTargets.push(toRepoRelativePath(TODO_PATH));
  }

  const shouldWriteSnapshotHistory =
    persistedSnapshotHistory == null ||
    !areSnapshotHistoriesEqual(nextSnapshotHistory, persistedSnapshotHistory);
  if (shouldWriteSnapshotHistory) {
    await writePersistedSnapshotHistory(nextSnapshotHistory);
    updatedTargets.push(toRepoRelativePath(SNAPSHOT_HISTORY_PATH));
  }

  if (updatedTargets.length > 0) {
    console.log(`updated ${updatedTargets.join(", ")}`);
  } else {
    console.log("unchanged performance dashboard");
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
