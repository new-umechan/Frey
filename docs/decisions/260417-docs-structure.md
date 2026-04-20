# ドキュメント分類方針

## Status

Accepted

## Context

このプロジェクトは開発途中であり、ドキュメント先行で設計と実装を進める。

そのため、次の種類の情報が同時に存在する。

- 全体像や設計思想
- 現在の採用済み仕様
- 未実装・未確定の設計案
- 外部文献や比較検討の材料
- 日常開発や検証の運用手順

これらを同じ場所に置くと、何が正本で、何が案で、何が調査メモかが曖昧になる。

## Decision

`docs/` は次のカテゴリに分ける。

- `concepts/`
  背景説明、全体像、設計思想
- `reference/`
  採用済み仕様の正本
- `proposal/`
  未実装、未確定、再設計中の案
- `operations/`
  現在の開発・テスト・ベンチ運用
- `research/`
  文献、比較、探索ログ
- `decisions/`
  採用済みの重要判断

運用原則は次の通りとする。

- `reference/` に未採用案を置かない
- `proposal/` にある文書は正本として扱わない
- proposal 採用後は `reference/` に反映する
- proposal が不採用でも原則削除せず、状態を残す
- `research/` は根拠や材料の置き場であり、採用済み仕様の正本にしない
- `operations/` には現在有効な手順だけを書く

## Consequences

利点:

- 現在の正本と将来案が分離される
- docs-first の設計フローを明示できる
- 同じ議論のやり直しを減らせる

コスト:

- 文書を移動・昇格させる運用が必要になる
- proposal と reference の境界判断を都度行う必要がある

## Notes

`proposal/` の各文書は少なくとも `Status` を持つ。
使用する値は `Draft`, `Accepted`, `Rejected`, `Superseded` とする。
