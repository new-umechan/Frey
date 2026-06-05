# Alpha Era Snapshot Bootstrap の採用

## Status

Superseded

## Context

`alpha` 検証では `Crust` から毎回積み上げるコストが大きく、最新モジュール確認の反復が遅くなる。
ただし simulation の意味変更や公開 API 破壊は避ける必要がある。

## Decision

かつては `alpha` 専用の開発用 era 境界 snapshot 復元経路を採用していた。

## Rationale

- bootstrap 短縮を `alpha` に限定することで、リスクと運用負担を抑えられる
- フォールバック前提で導入すれば、開発補助機能の失敗が通常検証を阻害しない

## Consequences

- 日常の `alpha` 検証で待機時間を短縮できる
- snapshot 形式と manifest の互換管理が増える
- 再生成トリガー管理と docs 同期が必要になる

## Superseded By

Server 化により、クライアント側の開発用 checkpoint jump と snapshot 復元経路は削除した。
以後の開発・検証 bootstrap は server/precompute 経路を使う。
