# Timeline Worker Protocol And Retention Budget

## Status

Superseded by `../reference/interface/wasm_api.md` and `../reference/architecture/data_model.md`

## 背景

`TimelineRuntime` 自体は branch / cursor / retention を持つようになったが、
worker protocol はまだ `exec_world_slice_and_delta` / `restore_world_to_tick` /
`fork_world` など旧語彙が残っている。

また retention policy も件数ベースだけで、
undo log / checkpoint がどの程度のメモリを占有しているかを runtime が説明できない。

## 目的

- worker / UI の操作語彙を timeline 中心へ寄せる
- retention policy に推定メモリ上限を加える
- `get_timeline_state` から checkpoint / undo log の window と推定メモリ量を取得できるようにする

## 提案

- `TimelineConfig` / `TimelineRetentionPolicy` に `max_estimated_bytes` を追加する
- `TimelineRuntime` は checkpoint / undo log の推定バイト数を計算できるようにする
- prune は `count limit` と `estimated byte limit` の両方で行う
- worker protocol / engine client に次の正本名を追加する
    - `advance_timeline`
    - `advance_timeline_slice`
    - `advance_timeline_slice_and_delta`
    - `get_view_delta`
    - `list_checkpoint_ticks`
    - `seek_world_to_tick`
    - `fork_timeline_branch`
    - `get_timeline_state`
- playback 側は `advance_timeline_slice_and_delta` を使い、
  UI は必要に応じて `get_timeline_state` を参照できるようにする

## スコープ

- `rust/src/application/world_dto.rs`
- `rust/src/application/world_runtime.rs`
- `web/src/app/engine/*.ts`
- `web/src/app/sim/world-stepper.ts`
- 関連 docs

## 成功条件

- retention が推定メモリ上限を持つ
- `TimelineStateResponse` に retention/usage 情報が含まれる
- worker protocol から旧 `history/restore/fork_world/get_world_delta` 依存が減る
- `pnpm app:test:run` と `cargo test --manifest-path rust/Cargo.toml application::world_` が通る

## リスクとトレードオフ

- 推定バイト数は厳密値ではなく近似になる
- worker request 名の変更は型定義やテストに広く波及する

ただし、逆再生前提の運用では「どれくらい保持しているか」を runtime が説明できない方が問題が大きい。
