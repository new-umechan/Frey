# Timeline API And Branch Runtime

## Status

Superseded

## Replaced by

`single-timeline-runtime-without-branch.md`

## 背景

現行実装は `TimelineRuntime` と `TickUndoLog` を持つが、
公開面はまだ `exec_world` / `restore_world_to_tick` 系の互換 API が中心で、
branch・cursor・retention が runtime の正式概念として露出していない。

このままでは、将来の逆再生 UI / worker は
「現在 world を前進実行し、必要時だけ seek する」発想から抜け切れない。

## 目的

- `advance / rewind / seek / fork` を timeline の正本 API として定義する
- branch を一級の runtime 情報として保持する
- checkpoint / undo log の保持戦略を runtime policy に寄せる
- `tick 完了境界` を時間操作モデルの公開前提として明文化する
- `river_downstream` のような複合構造にも compact undo を導入する

## 提案

- `TimelineRuntime` に次を追加する
    - `TimelineBranch`
    - `TimelineCursor`
    - `TimelineRetentionPolicy`
- `WorldService` は world ごとに branch id を発番し、fork 時に lineage を更新する
- 公開 DTO / WASM API は次を正本名とする
    - `advance_timeline`
    - `rewind_world_by_ticks`
    - `seek_world_to_tick`
    - `fork_timeline_branch`
    - `get_timeline_state`
- `TickUndoLog` は selected sparse patch だけでなく、
  `river_downstream` 用 compact patch を持てるようにする
- `TimelineStateResponse` を追加し、UI / worker が branch id、head tick、
  checkpoint window、undo log window、tick boundary model を取得できるようにする

## スコープ

- `rust/src/application/world_dto.rs`
- `rust/src/application/world_runtime.rs`
- `rust/src/application/world_service.rs`
- `rust/src/application/world_use_cases.rs`
- `rust/src/application/world_query_use_cases.rs`
- `rust/src/wasm_api/world_sim/api/*.rs`
- `docs/reference/interface/wasm_api.md`
- `docs/concepts/runtime_layers.md`
- `docs/reference/architecture/data_model.md`

## 成功条件

- branch id が init / seek / rewind / fork / timeline query で取得できる
- runtime が checkpoint / undo log retention policy を保持する
- `tick 完了境界` が API と docs の両方で明示される
- `river_downstream` 変更時に hydrology full clone へ即フォールバックしなくなる
- `application::world_` テストが通る

## リスクとトレードオフ

- 公開 DTO に branch / head / retention 情報が増える
- runtime 責務を増やすため、初期段階では use case と runtime の境界調整が必要になる
- `river_downstream` compact undo は clone より複雑になるが、
  branch-safe な短距離 rewind のメモリ効率が改善する

## 実施計画

1. branch / cursor / retention を `TimelineRuntime` に導入する
2. DTO と WASM API を timeline 中心の公開面へ更新する
3. `river_downstream` compact undo を追加する
4. query API と docs を timeline state 前提に更新する

## 未解決事項

- retention policy を init config だけで受けるか、将来 runtime 可変にするか
- `view delta` の include field 名と changed field catalog の統合範囲
