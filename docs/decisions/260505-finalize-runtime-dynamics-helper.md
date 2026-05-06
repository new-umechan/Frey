# finalize の runtime 補助状態差分を helper 化する

## Status

Accepted

## Context

`finalize_tick_undo_log` の末尾には、runtime 補助状態差分の
`then_some` パターンが残っている。

## Decision

- runtime 補助状態の差分を記録する helper を追加する
- `hydrology_dynamics` / `geology_dynamics` / `applied_intervention_seq` に適用する

## Consequences

利点:

- 末尾ロジックの重複が減る
- 差分記録の手順が統一される

コスト:

- helper 呼び出し経由で 1 段抽象化される
