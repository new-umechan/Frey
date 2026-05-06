# Population / Settlement / Polity / Conflict を Sparse Undo 化する

## Status

Accepted

## Decision

- `population` は `population` / `birth_rate` / `death_rate` を sparse patch 化する
- `settlement` は `urbanization` を sparse patch 化する
- `polity` は `polity_id` を sparse patch 化する
- `conflict` は `conflict_intensity` / `occupier_id` を sparse patch 化する

## Consequences

- full clone 発生率が下がり retention 効率が改善する
- `Option<PolityId>` を含む patch 適用が追加される
