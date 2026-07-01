import fs from "node:fs";
import path from "node:path";
import { collectMarkdownTargets, DEFAULT_REPO_ROOT } from "./markdown-targets.ts";

export interface Violation {
    path: string;
    line: number;
    ruleId: string;
    message: string;
}

const VALID_DECISION_STATUSES = new Set([
    "Draft",
    "Accepted",
    "Rejected",
    "Superseded",
]);

const DOCS_REFERENCE_PATTERN = /docs\/[A-Za-z0-9_./-]+\.md/g;
const STATUS_HEADING_PATTERN = /^## Status\s*$/m;
const ADR_FILENAME_PATTERN = /^\d{6}-[a-z0-9-]+\.md$/;
const DECISION_MAX_ACCEPTED_WORDS = 350;

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

function lintDecisionStatus(relativePath: string, text: string): Violation[] {
    if (!relativePath.startsWith("docs/decisions/")) {
        return [];
    }

    const headingMatch = STATUS_HEADING_PATTERN.exec(text);
    if (!headingMatch || headingMatch.index == null) {
        return [{
            path: relativePath,
            line: 1,
            ruleId: "decision-status-required",
            message: "missing `## Status` section",
        }];
    }

    const headingLine = getLineNumber(text, headingMatch.index);
    const lines = text.split("\n");
    const nextNonEmptyLine = findNextNonEmptyLine(lines, headingLine);

    if (!nextNonEmptyLine || !VALID_DECISION_STATUSES.has(nextNonEmptyLine.line)) {
        return [{
            path: relativePath,
            line: headingLine,
            ruleId: "decision-status-required",
            message: "invalid Status value; expected one of Draft, Accepted, Rejected, Superseded",
        }];
    }

    return [];
}

function getDecisionStatus(text: string): { status: string; line: number } | null {
    const headingMatch = STATUS_HEADING_PATTERN.exec(text);
    if (!headingMatch || headingMatch.index == null) {
        return null;
    }

    const headingLine = getLineNumber(text, headingMatch.index);
    const lines = text.split("\n");
    const nextNonEmptyLine = findNextNonEmptyLine(lines, headingLine);
    if (!nextNonEmptyLine) {
        return null;
    }

    return {
        status: nextNonEmptyLine.line,
        line: nextNonEmptyLine.lineNumber,
    };
}

function countWords(text: string): number {
    return text.split(/\s+/).filter((word) => word.length > 0).length;
}

function lintDecisionLifecycle(relativePath: string, text: string): Violation[] {
    if (!relativePath.startsWith("docs/decisions/")) {
        return [];
    }

    const status = getDecisionStatus(text);
    if (!status || !VALID_DECISION_STATUSES.has(status.status)) {
        return [];
    }

    const violations: Violation[] = [];

    if (status.status === "Draft" && !/^## Close when\s*$/m.test(text)) {
        violations.push({
            path: relativePath,
            line: status.line,
            ruleId: "decision-draft-close-when-required",
            message: "Draft decisions must define `## Close when`",
        });
    }

    if (status.status === "Accepted") {
        const wordCount = countWords(text);
        if (wordCount > DECISION_MAX_ACCEPTED_WORDS) {
            violations.push({
                path: relativePath,
                line: status.line,
                ruleId: "decision-accepted-compressed",
                message: `Accepted decisions must stay compressed (${wordCount}/${DECISION_MAX_ACCEPTED_WORDS} words)`,
            });
        }
    }

    if (status.status === "Superseded" && !/Superseded by|Replaced by|^## Superseded By\s*$/m.test(text)) {
        violations.push({
            path: relativePath,
            line: status.line,
            ruleId: "decision-superseded-target-required",
            message: "Superseded decisions must name the replacement target",
        });
    }

    return violations;
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
            message: "decision filename must match `YYMMDD-slug.md`",
        });
    }

    return violations;
}

export function lintRepo(repoRoot: string = DEFAULT_REPO_ROOT): Violation[] {
    const violations: Violation[] = [];

    for (const repoRelativePath of collectMarkdownTargets(repoRoot)) {
        const filePath = path.join(repoRoot, repoRelativePath);
        const text = fs.readFileSync(filePath, "utf8");

        violations.push(...lintDocsPathExists(repoRoot, repoRelativePath, text));
        violations.push(...lintDecisionStatus(repoRelativePath, text));
        violations.push(...lintDecisionLifecycle(repoRelativePath, text));
        violations.push(...lintReferenceStatus(repoRelativePath, text));
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
