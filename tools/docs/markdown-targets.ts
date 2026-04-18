import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const IGNORED_DIRECTORY_NAMES = new Set([
    "node_modules",
    "generated",
    "target",
]);

const IGNORED_FILE_NAMES = new Set([
    "AGENTS.md",
]);

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export const DEFAULT_REPO_ROOT = path.resolve(__dirname, "..", "..");

function toPosixPath(filePath: string): string {
    return filePath.split(path.sep).join("/");
}

function listMarkdownFiles(dirPath: string): string[] {
    if (!fs.existsSync(dirPath)) {
        return [];
    }

    const results: string[] = [];
    for (const entry of fs.readdirSync(dirPath, { withFileTypes: true })) {
        if (entry.isDirectory()) {
            if (IGNORED_DIRECTORY_NAMES.has(entry.name)) {
                continue;
            }

            results.push(...listMarkdownFiles(path.join(dirPath, entry.name)));
            continue;
        }

        if (!entry.isFile()) {
            continue;
        }

        if (!entry.name.endsWith(".md")) {
            continue;
        }

        if (IGNORED_FILE_NAMES.has(entry.name)) {
            continue;
        }

        results.push(path.join(dirPath, entry.name));
    }

    return results;
}

export function collectMarkdownTargets(repoRoot: string = DEFAULT_REPO_ROOT): string[] {
    return listMarkdownFiles(repoRoot)
        .map((filePath) => toPosixPath(path.relative(repoRoot, filePath)))
        .sort((left, right) => left.localeCompare(right));
}
