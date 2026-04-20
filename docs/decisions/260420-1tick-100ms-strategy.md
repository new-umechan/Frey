# 1tick=100ms 戦略の採用

## Status

Accepted

## Context

`1tick=100ms` 目標に対して、現状は約 `489ms/tick` である。
現行実装では `exec_world` の比率が高く、特に Hydrology 系処理と post-step の観測同期が支配的である。
また、UI 実行は Worker 分離済みだが、再生ループで `exec_world_slice` と `get_world_delta` が別往復になっている。

## Decision

`1tick=100ms` 改善は次の順序で実施する。

1. 最適化・リファクタ（間引きなし）
2. 決定性を保った並列化/パイプライン化
3. 最終手段として観測・描画のみ間引き

決定性の受け入れ基準は bitwise 一致ではなく、既存の seed 回帰ゲート準拠とする。

本決定に伴い、次を採用する。

- `WorldTransportCache.observe_world` の zero-allocation 方針
    - 毎tickの一時 `Vec` 生成を避け、`observe_with` 経由で shadow を直接比較更新する。
- Worker パイプライン化
    - `exec_world_slice_and_delta` を導入し、`exec_world_slice` 後の `get_world_delta` を同一 worker 要求に束ねる。

## Rationale

- 大きな計算モジュールを間引かずに、まず固定コストとIPC往復を削るほうが安全に速度を稼げる。
- seed 回帰ゲートは既存運用に組み込み済みで、決定性回帰を自動検知できる。
- Worker 内で順序を固定したまま処理するため、同一入力での再現性を維持しやすい。

## Consequences

- `world_runtime` の実装は増えるが、post-step 観測の割当負荷が下がる。
- Engine worker protocol に内部API (`exec_world_slice_and_delta`) が増える。
- 公開 wasm API シグネチャは変更しない。
- 100ms 未達時は次段で WASM threads 導入可否を別 decision で評価する。
