# Alpha Era Snapshot Bootstrap の採用

## Status

Accepted

## Context

`alpha` 検証では `Crust` から毎回積み上げるコストが大きく、最新モジュール確認の反復が遅くなる。
ただし simulation の意味変更や公開 API 破壊は避ける必要がある。

## Decision

次を採用する。

- `alpha` 専用・dev 専用の era 境界 snapshot 復元経路を導入する
- snapshot 正本は `./.cache/frey/alpha-snapshots/`、browser 読込は `web/public/.dev-precomputed/alpha/` mirror を使う
- 対象 stage は `environment(800)`, `life(1300)`, `civilization(1395)`, `history(1445)` とする
- 復元は opt-in (`FREY_DEV_SNAPSHOT_STAGE` / `devSnapshotStage`) のときのみ有効化する
- 不在・破損・fingerprint 不一致は warning の上で通常計算へフォールバックする

## Rationale

- bootstrap 短縮を `alpha` に限定することで、リスクと運用負担を抑えられる
- mirror 分離により browser 側の読込経路を単純化できる
- フォールバック前提で導入すれば、開発補助機能の失敗が通常検証を阻害しない

## Consequences

利点:

- 日常の `alpha` 検証で待機時間を短縮できる
- `seed != alpha` の既存挙動を維持できる

コスト:

- snapshot 形式と manifest の互換管理が増える
- 再生成トリガー管理と docs 同期が必要になる
