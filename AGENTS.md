## Rules

- Commit message: `tag: 日本語の説明`（例: `feat: ログイン機能の追加`）
- Do not run `git push`.
- Use `pnpm`, not `npm`.
- Keep `mod.rs` thin: only re-exports and submodule declarations.
- Assume general programming knowledge, but explain repository-specific concepts.
- Simulation models must be grounded in scientific literature. Approximations are allowed for performance, but document trade-offs.

## Docs-First Development

For non-trivial design or behavior changes:

1. Draft unresolved designs in `docs/proposal/`.
2. Record adopted/rejected important decisions in `docs/decisions/`.
3. Implement.
4. Update canonical docs:
   - `docs/reference/`: implemented specs
   - `docs/operations/`: current procedures
   - `docs/operations/bench/`: benchmark methods, validation logs, comparisons, rejected hypotheses

Skip proposal/decision docs for small refactors, local cleanups, test-only changes, minor naming fixes, or behavior already covered by canonical docs.

## Documentation

- `docs/research/`: external research and pre-decision material
- `docs/proposal/`: future-facing design, never canonical
- `docs/decisions/`: adopted/rejected decisions and reasons
- `docs/reference/`: implementation-matching specs only; no history or tentative plans
- `docs/operations/`: current procedures only
- `docs/operations/bench/`: benchmark procedures, validation logs, comparison history, rejected hypotheses
- Remove or replace stale text; do not just append.
