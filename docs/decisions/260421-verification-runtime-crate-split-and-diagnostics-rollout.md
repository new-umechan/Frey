# Verification Runtime crate分離と diagnostics 段階導入

## Status

Accepted

## Context

`docs/proposal/verification-runtime-redesign.md` では、検証実行系の再編方針は採用済みだが、
次の実装境界が未確定だった。

- `verification runtime` を `application` 内に置くか、別 crate に分離するか
- module diagnostics 統一をどの順序で進めるか

このまま実装を進めると、責務境界と移行順序がブレるため、実装フェーズ向けに固定する。

## Decision

次を採用する。

- この実装フェーズで `verification runtime` を別 crate に分離する
    - `application` 直下の補助 module ではなく、検証実行面の責務を crate 境界で分離する
    - runner / baseline compare / tolerance policy / perf probe を同 crate に集約する
- module diagnostics 統一は staged rollout で進め、初手を
  `Geology -> Climate -> Hydrology` の順で実施する
    - 初期3モジュールで指標定義と運用を安定化してから他モジュールへ展開する

## Consequences

利点:

- 実行責務が crate 境界で明確になり、検証系変更が `application` 実装へ波及しにくくなる
- diagnostics 統一を高コスト領域から先に進められ、perf/regression の共通運用を早く固められる

コスト:

- crate 分離に伴う依存整理と API 境界の設計コストが増える
- 段階導入中は diagnostics の適用範囲がモジュール間で一時的に不均一になる
