# 260506 Polity Groups Structured Undo

## Status

Accepted

## Context

`relations` の undo は map については before-value patch 化できていたが、
`polity_groups` は変更時に before 側の `Vec<PolityGroup>` 全体を保持していた。

単一 timeline / retention budget 前提では、この full snapshot が
group 数の増加に対して直線的に効いてしまう。

## Decision

- `polity_groups` は `PolityGroupsUndoState` で扱う
- 変更 group の before payload は `upserts` に保存する
- after 側で新規作成された group id は `removals` に保存する
- before 側の順序は `order_before` として保存し、undo 適用時に再構成する

## Consequences

- `relations` の undo は map と group vector の両方で structured になる
- full snapshot より保持量を抑えつつ、before 側の順序まで exact restore できる
