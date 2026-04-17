import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export interface Violation {
    path: string;
    line: number;
    ruleId: string;
    message: string;
}

const VALID_PROPOSAL_STATUSES = new Set([
    "Draft",
    "Accepted",
    "Rejected",
    "Superseded",
]);

const DOCS_REFERENCE_PATTERN = /docs\/[A-Za-z0-9_./-]+\.md/g;
const STATUS_HEADING_PATTERN = /^## Status\s*$/m;
const ADR_FILENAME_PATTERN = /^ADR-\d{4}-[a-z0-9-]+\.md$/;

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const DEFAULT_REPO_ROOT = path.resolve(__dirname, "..", "..");

function toPosixPath(filePath: string): string {
    return filePath.split(path.sep).join("/");
}

function listMarkdownFiles(dirPath: string): string[] {
    if (!fs.existsSync(dirPath)) {
        return [];
    }

    const results: string[] = [];
    for (const entry of fs.readdirSync(dirPath, { withFileTypes: true })) {
        const entryPath = path.join(dirPath, entry.name);
        if (entry.isDirectory()) {
            results.push(...listMarkdownFiles(entryPath));
            continue;
        }

        if (entry.isFile() && entry.name.endsWith(".md")) {
            results.push(entryPath);
        }
    }

    return results;
}

function collectTargetMarkdownFiles(repoRoot: string): string[] {
    const docsDir = path.join(repoRoot, "docs");
    const files = listMarkdownFiles(docsDir);
    const rootReadme = path.join(repoRoot, "README.md");

    if (fs.existsSync(rootReadme)) {
        files.push(rootReadme);
    }

    return files.sort((left, right) => left.localeCompare(right));
}

function getLineNumber(text: string, index: number): number {
    return text.slice(0, index).split("\n").length;
}

function findNextNonEmptyLine(lines: string[], startIndex: number): { line: string; lineNumber: number } | null {
    for (let index = startIndex; index < lines.length; index += 1) {
        const trimmed = lines[index].trim();
        if (trimmed.length === 0) {
            continue;
        }

        return {
            line: trimmed,
            lineNumber: index + 1,
        };
    }

    return null;
}

function lintDocsPathExists(repoRoot: string, relativePath: string, text: string): Violation[] {
    const violations: Violation[] = [];

    for (const match of text.matchAll(DOCS_REFERENCE_PATTERN)) {
        const refPath = match[0];
        const absoluteRefPath = path.join(repoRoot, refPath);
        if (fs.existsSync(absoluteRefPath)) {
            continue;
        }

        violations.push({
            path: relativePath,
            line: getLineNumber(text, match.index ?? 0),
            ruleId: "docs-path-exists",
            message: `missing docs reference: ${refPath}`,
        });
    }

    return violations;
}

function lintProposalStatus(relativePath: string, text: string): Violation[] {
    if (!relativePath.startsWith("docs/proposal/")) {
        return [];
    }

    const headingMatch = STATUS_HEADING_PATTERN.exec(text);
    if (!headingMatch || headingMatch.index == null) {
        return [{
            path: relativePath,
            line: 1,
            ruleId: "proposal-status-required",
            message: "missing `## Status` section",
        }];
    }

    const headingLine = getLineNumber(text, headingMatch.index);
    const lines = text.split("\n");
    const nextNonEmptyLine = findNextNonEmptyLine(lines, headingLine);

    if (!nextNonEmptyLine || !VALID_PROPOSAL_STATUSES.has(nextNonEmptyLine.line)) {
        return [{
            path: relativePath,
            line: headingLine,
            ruleId: "proposal-status-required",
            message: "invalid Status value; expected one of Draft, Accepted, Rejected, Superseded",
        }];
    }

    return [];
}

function lintReferenceStatus(relativePath: string, text: string): Violation[] {
    if (!relativePath.startsWith("docs/reference/")) {
        return [];
    }

    const headingMatch = STATUS_HEADING_PATTERN.exec(text);
    if (!headingMatch || headingMatch.index == null) {
        return [];
    }

    return [{
        path: relativePath,
        line: getLineNumber(text, headingMatch.index),
        ruleId: "reference-status-forbidden",
        message: "reference docs must not define `## Status`",
    }];
}

function lintDecisionFilename(repoRoot: string): Violation[] {
    const decisionsDir = path.join(repoRoot, "docs", "decisions");
    if (!fs.existsSync(decisionsDir)) {
        return [];
    }

    const violations: Violation[] = [];
    for (const entry of fs.readdirSync(decisionsDir, { withFileTypes: true })) {
        if (!entry.isFile() || !entry.name.endsWith(".md")) {
            continue;
        }

        if (ADR_FILENAME_PATTERN.test(entry.name)) {
            continue;
        }

        violations.push({
            path: `docs/decisions/${entry.name}`,
            line: 1,
            ruleId: "decision-filename-format",
            message: "decision filename must match `ADR-XXXX-slug.md`",
        });
    }

    return violations;
}

export function lintRepo(repoRoot: string = DEFAULT_REPO_ROOT): Violation[] {
    const violations: Violation[] = [];

    for (const filePath of collectTargetMarkdownFiles(repoRoot)) {
        const relativePath = toPosixPath(path.relative(repoRoot, filePath));
        const text = fs.readFileSync(filePath, "utf8");

        violations.push(...lintDocsPathExists(repoRoot, relativePath, text));
        violations.push(...lintProposalStatus(relativePath, text));
        violations.push(...lintReferenceStatus(relativePath, text));
    }

    violations.push(...lintDecisionFilename(repoRoot));

    return violations.sort((left, right) => {
        if (left.path !== right.path) {
            return left.path.localeCompare(right.path);
        }
        if (left.line !== right.line) {
            return left.line - right.line;
        }
        return left.ruleId.localeCompare(right.ruleId);
    });
}

export function formatViolations(violations: Violation[]): string[] {
    return violations.map((violation) =>
        `${violation.path}:${violation.line}: ${violation.ruleId}: ${violation.message}`,
    );
}

export function runDocsLint(
    repoRoot: string = DEFAULT_REPO_ROOT,
    write: (line: string) => void = console.log,
): number {
    const violations = lintRepo(repoRoot);
    for (const line of formatViolations(violations)) {
        write(line);
    }
    write(`${violations.length} error(s)`);
    return violations.length === 0 ? 0 : 1;
}

if (import.meta.url === `file://${process.argv[1]}`) {
    process.exit(runDocsLint());
}
