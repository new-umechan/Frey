import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { lintRepo, runDocsLint } from "../../../tools/docs/docs-lint";

function makeTempRepo(): string {
    return fs.mkdtempSync(path.join(os.tmpdir(), "frey-docs-lint-"));
}

function writeFile(repoRoot: string, relativePath: string, content: string): void {
    const filePath = path.join(repoRoot, relativePath);
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(filePath, content);
}

const tempDirs: string[] = [];

afterEach(() => {
    for (const dir of tempDirs.splice(0)) {
        fs.rmSync(dir, { recursive: true, force: true });
    }
});

describe("docs-lint", () => {
    it("passes when docs references and metadata are valid", () => {
        const repoRoot = makeTempRepo();
        tempDirs.push(repoRoot);

        writeFile(repoRoot, "README.md", "see docs/README.md\n");
        writeFile(repoRoot, "docs/README.md", "# docs\n");
        writeFile(repoRoot, "docs/proposal/idea.md", "# Proposal\n\n## Status\n\nDraft\n");
        writeFile(repoRoot, "docs/reference/spec.md", "# Reference\n");
        writeFile(repoRoot, "docs/decisions/260417-docs-structure.md", "# ADR\n");

        expect(lintRepo(repoRoot)).toEqual([]);
    });

    it("reports missing docs path references with the source line number", () => {
        const repoRoot = makeTempRepo();
        tempDirs.push(repoRoot);

        writeFile(repoRoot, "README.md", "line1\nsee docs/missing.md\n");
        writeFile(repoRoot, "docs/README.md", "# docs\n");

        expect(lintRepo(repoRoot)).toEqual([
            expect.objectContaining({
                path: "README.md",
                line: 2,
                ruleId: "docs-path-exists",
            }),
        ]);
    });

    it("requires a valid Status section for proposal docs", () => {
        const repoRoot = makeTempRepo();
        tempDirs.push(repoRoot);

        writeFile(repoRoot, "README.md", "# root\n");
        writeFile(repoRoot, "docs/proposal/idea.md", "# Proposal\n\n## Status\n\nPending\n");

        expect(lintRepo(repoRoot)).toEqual([
            expect.objectContaining({
                path: "docs/proposal/idea.md",
                line: 3,
                ruleId: "proposal-status-required",
            }),
        ]);
    });

    it("forbids Status sections in reference docs", () => {
        const repoRoot = makeTempRepo();
        tempDirs.push(repoRoot);

        writeFile(repoRoot, "README.md", "# root\n");
        writeFile(repoRoot, "docs/reference/spec.md", "# Ref\n\n## Status\n\nAccepted\n");

        expect(lintRepo(repoRoot)).toEqual([
            expect.objectContaining({
                path: "docs/reference/spec.md",
                line: 3,
                ruleId: "reference-status-forbidden",
            }),
        ]);
    });

    it("validates decision filenames", () => {
        const repoRoot = makeTempRepo();
        tempDirs.push(repoRoot);

        writeFile(repoRoot, "README.md", "# root\n");
        writeFile(repoRoot, "docs/decisions/adr-1-docs.md", "# ADR\n");

        expect(lintRepo(repoRoot)).toEqual([
            expect.objectContaining({
                path: "docs/decisions/adr-1-docs.md",
                line: 1,
                ruleId: "decision-filename-format",
            }),
        ]);
    });

    it("returns exit code 1 and prints plain text violations", () => {
        const repoRoot = makeTempRepo();
        tempDirs.push(repoRoot);

        writeFile(repoRoot, "README.md", "docs/missing.md\n");

        const lines: string[] = [];
        const exitCode = runDocsLint(repoRoot, (line) => lines.push(line));

        expect(exitCode).toBe(1);
        expect(lines).toEqual([
            "README.md:1: docs-path-exists: missing docs reference: docs/missing.md",
            "1 error(s)",
        ]);
    });

    it("returns exit code 0 when there are no violations", () => {
        const repoRoot = makeTempRepo();
        tempDirs.push(repoRoot);

        writeFile(repoRoot, "README.md", "docs/README.md\n");
        writeFile(repoRoot, "docs/README.md", "# docs\n");

        const lines: string[] = [];
        const exitCode = runDocsLint(repoRoot, (line) => lines.push(line));

        expect(exitCode).toBe(0);
        expect(lines).toEqual(["0 error(s)"]);
    });
});
