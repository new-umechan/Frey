# SoA + EntityState の採用

## Date

2026-04-17

## Status

Accepted

## Context

このシミュレータの処理の本質は「全セル（約4万）に対して、同じ計算を一斉に適用する」ことである。
セル状態には SoA が適合し、CPU キャッシュ効率を最大化できる。

また、Tier 2 モジュール追加時に `Module` と必要な `System`・Component を登録するだけで拡張できるため、
複雑性の増加に対してアーキテクチャが崩れにくい。

## Decision

セルと非セル Entity で管理方法を分ける。

- `CellStore`（自前 SoA）
  - 全セルの現在値 Component を保持する
- `EntityState`（疎な Entity）
  - `slotmap` ベースで Polity・Settlement・Region などを保持する
- `polity_relations`（国家間関係）
  - 国家間の重み付き関係を保持する

データ配置と型定義の詳細は `docs/reference/architecture/data_model.md` を参照する。

## Consequences

- セル向けの一斉計算を SoA で最適化しやすくなる
- 疎な Entity を `slotmap` 系の管理に切り出せる
- データ配置の責務が分かれ、実装拡張時に構造が崩れにくくなる
