# VerificationMode 導入と Finalize 完了時 post-step 統合

## Status

Accepted

## Context

`seed regression` の native 化（phase 1）で待ち時間は短縮したが、実行系には次の混在が残っていた。

- `simulation verification` と `presentation/perf verification` の post-step 処理が同経路で混在
- `pending_post_step` による別処理が slice 実行の理解と profiling を難しくする
- CI gate は native を正本にしつつ、WASM回帰の補助運用を明示する必要がある

## Decision

次を採用する。

- `VerificationMode` を導入する
  - `Interactive`
  - `HeadlessMetrics`
  - `ScientificBenchmark`
- `HeadlessMetrics` では post-step の重い処理を停止する
  - `post_step_sync_light`
  - `observe_after_world_change`
  - `history snapshot`
- `pending_post_step` は廃止し、`Finalize` 完了時に同ループ内で post-step を即時実行する
- `ScientificBenchmark` は `Interactive` 相当実行に加えて、初版フックとして
  tick単位の `WorldMetrics` サンプルを蓄積する
- gate は native を正本とし、WASM補助ゲートは `test:gate:regression:wasm` として分離する
  - 通常の `test:gate`/常時CIゲートには含めない
  - 手動実行用workflowを分離する（`regression-wasm-support-gate.yaml`）

## Consequences

利点:

- verification mode ごとに post-step コストを明確に切り替えられる
- slice 実行の phase model と実際の tick 完了処理が一致する
- native と WASM の gate 役割が明確になり、失敗原因の切り分けがしやすい

コスト:

- mode 分岐により runtime の条件分岐が増える
- `ScientificBenchmark` 初版は `Interactive` 同等であり、詳細artifact収集は後続実装が必要
