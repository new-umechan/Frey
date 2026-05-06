# Single Timeline Runtime Without Branch

## Status

Accepted

## 背景

逆再生前提で runtime を再設計するうえで、
時間軸を一級に扱う方針自体は妥当だったが、
branch を timeline の正式概念に入れると責務が過剰になる。

このプロジェクトでは、ひとまず UI を変えず、
世界の編集操作も timeline 分岐も導入しない。
必要なのは「単一の時間軸を前後に移動できる runtime」であって、
複数 future を管理する timeline graph ではない。

## 目的

- `TimelineRuntime` を branch なし単一 timeline の正本にする
- `current_tick` と `head_tick` を分離し、cursor と計算済み範囲を明示する
- `advance / rewind / seek` を単一 timeline 上の移動として再定義する
- `checkpoint` と `TickUndoLog` を高速化用の補助構造として位置づける
- `tick 完了境界` を唯一の公開整合点として固定する
- UI は変えず、application/runtime/worker の責務だけを再編する

## 提案

- `TimelineRuntime` は次だけを持つ
    - `TimelineCursor`
    - `TimelineArchive`
    - `TickUndoLog store`
    - `TimelineRetentionPolicy`
- branch 系 state は削除する
    - `TimelineBranch`
    - `branch_id`
    - `parent_branch_id`
    - `forked_from_tick`
- `advance_timeline` は `head_tick` を超えるぶんだけ新規計算する
- `seek_world_to_tick` と `rewind_world_by_ticks` は cursor 移動として扱い、
  未来側 checkpoint / undo log は破棄しない
- `fork_timeline_branch` / `fork_world` は正式 API から外す
- worker / WASM API / DTO から branch metadata を外す

## スコープ

- `rust/src/application/world_dto.rs`
- `rust/src/application/world_runtime.rs`
- `rust/src/application/world_service.rs`
- `rust/src/application/world_use_cases.rs`
- `rust/src/application/world_query_use_cases.rs`
- `rust/src/wasm_api/world_sim/api/*.rs`
- `web/src/app/engine/*.ts`
- `docs/reference/interface/wasm_api.md`
- `docs/concepts/runtime_layers.md`
- `docs/reference/architecture/data_model.md`

## 成功条件

- `TimelineRuntime` が branch metadata なしで動作する
- `advance / rewind / seek / get_timeline_state / get_view_delta` が正本 API になる
- `seek` や `rewind` 後も `head_tick` より先の履歴を保持し続ける
- DTO / worker protocol から branch 系フィールドが消える
- UI 変更なしで `application::world_` テストと build が通る

## リスクとトレードオフ

- fork を消すため、複数 timeline を同時管理する用途は一旦扱わない
- `seek` の実装は checkpoint / replay と undo 最適化の両立が必要になる
- `head_tick` までの既存履歴を再利用するため、
  runtime に「現在位置」と「計算済み最前位置」の両方を明示的に持たせる必要がある

## 実施計画

1. branch 前提 docs を superseded にする
2. runtime / DTO / use case から branch / fork / future discard を外す
3. worker / WASM API を単一 timeline 語彙へ寄せる
4. architecture docs を単一 timeline モデルに更新する

## 未解決事項

- `seek(target_tick > head_tick)` の内部経路を専用最適化するか、
  `advance_timeline` の再利用に寄せるか
- 将来 intervention を入れる場合に `head_tick` より未来の扱いをどう契約化するか
