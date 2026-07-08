## Rules

- Commit messages must be `tag: 日本語の説明` format, e.g. `feat: ログイン機能の追加`.
- Do not run `git push`.
- Use `pnpm`, not `npm`.
- Keep `mod.rs` thin: only submodule declarations and re-exports.
- Assume general programming knowledge; explain repository-specific concepts.
- Simulation models must be anchored in scientific literature. Performance approximations are allowed, but document what is approximated and where the trade-off is recorded.

## Documentation Workflow

For non-trivial design or behavior changes:

1. Write unresolved designs as `Draft` decision docs in `docs/decisions/`.
2. Update decision status when adopted, rejected, or superseded.
3. Implement the change.
4. Update canonical docs.

Decision docs are not required for small refactors, local cleanups, test-only changes, naming fixes, or behavior already covered by canonical docs.

## Docs Directory Guide

- `docs/research/`: external research and pre-decision material.
- `docs/decisions/`: important decisions, their status, and reasoning.
- `docs/reference/`: current behavior, model contracts, module responsibilities, and public contracts.
- `docs/operations/`: current procedures.
- `docs/operations/bench/`: benchmark methods, validation logs, comparisons, diagnostics, and rejected hypotheses.

Keep docs current by replacing stale text instead of only appending.
