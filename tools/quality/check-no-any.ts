import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, "..", "..");

const TARGET_DIRS = [
    path.join(ROOT_DIR, "web", "src", "app"),
    path.join(ROOT_DIR, "web", "src", "gfx"),
    path.join(ROOT_DIR, "web", "src", "components"),
];

const FILE_EXTENSIONS = new Set([".ts", ".tsx", ".d.ts"]);
const ANY_PATTERN = /\bany\b/;

type Hit = {
    filePath: string;
    lineNo: number;
    lineText: string;
};

function walkFiles(dirPath: string): string[] {
    const entries = fs.readdirSync(dirPath, { withFileTypes: true });
    const files: string[] = [];
    for (const entry of entries) {
        const absPath = path.join(dirPath, entry.name);
        if (entry.isDirectory()) {
            files.push(...walkFiles(absPath));
            continue;
        }
        if (!entry.isFile()) {
            continue;
        }
        if (!FILE_EXTENSIONS.has(path.extname(entry.name))) {
            continue;
        }
        files.push(absPath);
    }
    return files;
}

function collectAnyHits(filePath: string): Hit[] {
    const lines = fs.readFileSync(filePath, "utf8").split(/\r?\n/);
    const hits: Hit[] = [];
    for (let i = 0; i < lines.length; i += 1) {
        const lineText = lines[i];
        if (!ANY_PATTERN.test(lineText)) {
            continue;
        }
        hits.push({
            filePath: path.relative(ROOT_DIR, filePath),
            lineNo: i + 1,
            lineText: lineText.trim(),
        });
    }
    return hits;
}

function main() {
    const targetFiles = TARGET_DIRS.flatMap((dirPath) => walkFiles(dirPath));
    const hits = targetFiles.flatMap((filePath) => collectAnyHits(filePath));

    if (hits.length === 0) {
        console.log("PASS: no `any` found in web/src/app, web/src/gfx, web/src/components");
        return;
    }

    console.error("FAIL: `any` usage found");
    for (const hit of hits) {
        console.error(`${hit.filePath}:${hit.lineNo}: ${hit.lineText}`);
    }
    process.exitCode = 1;
}

main();
