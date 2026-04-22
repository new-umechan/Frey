# VerificationMode 導入と Finalize 完了時 post-step 統合

## Status

Accepted

## Context

`seed regression` の native 化（phase 1）で待ち時間は短縮したが、実行系には次の混在が残っていた。

- `simulation verification` と `presentation/perf verification` の post-step 処理が同経路で混在
- `pending_post_step` による別処理が slice 実行の理解と profiling を難しくする
- `ScientificBenchmark` artifact の保存先が未確定で、再現性と共有運用の方針が曖昧
- perf gate の評価経路を単一路線に寄せると、劣化箇所の切り分けが難しい

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
- `ScientificBenchmark` は `Interactive` 相当実行に加えて、
    tick単位の `WorldMetrics` サンプルと比較用 artifact を収集する
    - 保存先は CI artifact とリポジトリ内ファイルの両方を必須とする
- seed regression gate は native を正本とし、WASM補助ゲートは `test:gate:regression:wasm` として分離する
    - 通常の `test:gate`/常時CIゲートには含めない
    - 手動実行用workflowを分離する（`regression-wasm-support-gate.yaml`）
- perf gate は `native + wasm + worker` の3レーンを必須にし、全レーン成功を合格条件にする

## Consequences

利点:

- verification mode ごとに post-step コストを明確に切り替えられる
- slice 実行の phase model と実際の tick 完了処理が一致する
- seed regression gate と perf gate で責務を分離しつつ、perf は3レーン比較で切り分けできる
- `ScientificBenchmark` artifact を CI とリポジトリ双方に残せるため、再現と参照が容易になる

コスト:

- mode 分岐により runtime の条件分岐が増える
- CI 実行時間と artifact 管理コストが増える
