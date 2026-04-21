# Native Seed Regression Runner の採用

## Status

Accepted

## Context

`seed regression` は日常開発で最も頻繁に回す検証の1つである。
一方、現行の主経路は `WASM build -> Node/TS -> WASM controller` に依存しており、
シミュレーション回帰確認としては transport/UI 都合を含みすぎている。

この構成には次の問題がある。

- 日常確認の待ち時間が長い
- build 失敗と simulation 回帰失敗の切り分けが弱い
- `simulation verification` と `interface verification` の責務が混ざる

検証実行系再設計 proposal では、まず `simulation verification` を Rust native に寄せる段階導入を行う。

## Decision

phase 1 として、`seed regression` の常用経路に Rust native runner を導入する。

次を採用する。

- `seed regression quick/heavy` は Rust binary を正本実行経路とする
- JSON 出力形式、baseline 形式、threshold ルールは既存 TS 実装と互換にする
- WASM 経路の `seed regression` は互換・比較用の補助経路として残す
- `WASM API contract` と `transport integration` は引き続き別ゲートとして扱う

## Rationale

- seed 回帰で見たいのは、world 更新の決定性と許容差分であり、WASM binding の正否ではない
- baseline 形式を維持すれば、既存運用資産を壊さずに native 化できる
- phase 1 を narrow に切ることで、`application` 層の feature 境界や verification runtime 全体再編を急がずに済む

## Consequences

利点:

- 日常の回帰確認で WASM build を必須にしない
- simulation regression と interface regression を切り分けやすくなる
- 将来の `verification runtime` 導入に向けて、native runner を足場にできる

コスト:

- 一時的に TS runner と Rust runner の2系統を持つ
- world 初期化と baseline 比較のロジックに重複が生じる
- `pending_post_step` や `VerificationMode` の整理は後続実装に残る
