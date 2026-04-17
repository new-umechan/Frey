import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const DEFAULT_REPO_ROOT = path.resolve(__dirname, "..", "..");
const BENCH_DIR = "docs/operations/bench";
const BENCH_README_PATH = `${BENCH_DIR}/README.md`;

function toPosixPath(filePath: string): string {
    return filePath.split(path.sep).join("/");
}

function listModuleDirectories(benchRoot: string): string[] {
    if (!fs.existsSync(benchRoot)) {
        return [];
    }

    return fs.readdirSync(benchRoot, { withFileTypes: true })
        .filter((entry) => entry.isDirectory() && !entry.name.startsWith("."))
        .map((entry) => entry.name)
        .sort((left, right) => left.localeCompare(right));
}

function listMarkdownFiles(dirPath: string): string[] {
    if (!fs.existsSync(dirPath)) {
        return [];
    }

    return fs.readdirSync(dirPath, { withFileTypes: true })
        .filter((entry) => entry.isFile() && entry.name.endsWith(".md"))
        .map((entry) => entry.name)
        .sort((left, right) => left.localeCompare(right));
}

function toModuleLabel(dirName: string): string {
    if (dirName.length === 0) {
        return dirName;
    }

    return `${dirName[0].toUpperCase()}${dirName.slice(1)}`;
}

function renderBenchReadme(repoRoot: string): string {
    const benchRoot = path.join(repoRoot, BENCH_DIR);
    const moduleDirs = listModuleDirectories(benchRoot);

    const lines: string[] = [];
    lines.push("# Bench Docs");
    lines.push("");
    lines.push("この文書は `pnpm docs:generate` による自動生成ファイルである。");
    lines.push("");
    lines.push("`docs/operations/bench/` 配下のモジュール別ベンチ文書を列挙する。");
    lines.push("");
    lines.push("## Modules");
    lines.push("");

    for (const moduleDir of moduleDirs) {
        const moduleLabel = toModuleLabel(moduleDir);
        lines.push(`### \`${moduleDir}/\` (${moduleLabel})`);
        lines.push("");

        const markdownFiles = listMarkdownFiles(path.join(benchRoot, moduleDir));
        if (markdownFiles.length === 0) {
            lines.push("- (no markdown files)");
            lines.push("");
            continue;
        }

        for (const fileName of markdownFiles) {
            const filePath = toPosixPath(path.join(BENCH_DIR, moduleDir, fileName));
            lines.push(`- \`${filePath}\``);
        }
        lines.push("");
    }

    lines.push("## References");
    lines.push("");
    lines.push("- `docs/operations/benchmark.md`");
    lines.push("- `docs/operations/test.md`");
    lines.push("");

    return lines.join("\n");
}

function writeFileIfChanged(targetPath: string, content: string): boolean {
    const nextContent = content.endsWith("\n") ? content : `${content}\n`;
    if (fs.existsSync(targetPath)) {
        const prev = fs.readFileSync(targetPath, "utf8");
        if (prev === nextContent) {
            return false;
        }
    }

    fs.mkdirSync(path.dirname(targetPath), { recursive: true });
    fs.writeFileSync(targetPath, nextContent);
    return true;
}

export function runDocsGenerate(
    repoRoot: string = DEFAULT_REPO_ROOT,
    write: (line: string) => void = console.log,
): number {
    const benchReadme = renderBenchReadme(repoRoot);
    const targetPath = path.join(repoRoot, BENCH_README_PATH);
    const changed = writeFileIfChanged(targetPath, benchReadme);
    write(changed ? `updated ${BENCH_README_PATH}` : `unchanged ${BENCH_README_PATH}`);
    return 0;
}

if (import.meta.url === `file://${process.argv[1]}`) {
    process.exit(runDocsGenerate());
}
