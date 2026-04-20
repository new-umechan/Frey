# 1tick=100ms 達成ロードマップ

## Status

Accepted

採用済み。段階戦略で実装を進める。実装済みの決定事項は `docs/decisions/260420-1tick-100ms-strategy.md` を参照する。

## 背景

- 現状の `tick_total.mean` は約 `489ms/tick` で、目標の `100ms/tick` に対して約 4.9 倍の差がある。
- 直近の perf 記録では `exec_world` が支配的で、特に `step_geology_river` と post-step 観測コストが大きい。
- 既存 UI は Worker 分離済みだが、tick 実行後の同期が複数往復になっている。

## 目的

- 間引きに頼らず、`tick_total.mean <= 100ms` を目指す。
- 決定性は seed 回帰ゲートの許容範囲内で維持する。
- 科学モデルの近似導入時は、根拠文献と計算コスト削減効果を文書化する。

## 提案概要

- 優先順は次の 3 段階とする。
  - 1. Rust 実行コアの最適化・リファクタ
  - 2. 決定性を崩さない Worker パイプライン化
  - 3. 最終手段として観測・描画経路のみ間引き
- 本変更では段階 1 と段階 2 の初期実装を入れる。
  - `WorldTransportCache.observe_world` で毎tick発生していた一時 `Vec` 生成を削減する。
  - `exec_world_slice` と `get_world_delta` を Worker 内で1回の要求にまとめる。

## 成功条件

- `bench:perf:record` で `exec_world.mean` と `tick_total.mean` が悪化しない。
- `test:seed:gate:quick` が通り、決定性ゲートを維持する。
- 既存 wasm API の公開シグネチャを変更しない。

## リスクとトレードオフ

- `observe_world` の zero-allocation 化は実装複雑性を上げるが、GC/割当コストを削減できる。
- Worker 1往復化はレイテンシを下げるが、Worker 内処理の責務が増える。
- 同一 world の tick 依存は維持するため、tick N と tick N+1 の同時実行は行わない。

## 実施計画

1. Docs-first: 本 proposal と decision を更新。
2. Rust 最適化: transport cache 観測の一時配列を削減。
3. Web パイプライン: `exec_world_slice_and_delta` を導入し IPC 往復を短縮。
4. 回帰確認: perf / seed gate を実行。
5. reference 更新: データモデル文書へ Managed 層の運用原則を反映。

## 未解決事項

- 100ms 未達時に WASM threads (`SharedArrayBuffer + atomics`) を採用するか。
- 間引きを導入する場合の閾値と適用範囲（観測・描画のみ）をどこまで許容するか。
