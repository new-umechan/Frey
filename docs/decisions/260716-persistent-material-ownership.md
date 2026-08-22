# Persistent material ownership を標準方式に固定する

## Status

Superseded

Superseded by `260822-topology-preserving-material-front.md`.

## Context

プレート所属の更新について、次の方式を比較した。

- influence field による再分類
- セル内の複数 material の再構成
- Euler front の直接移動
- 共有 boundary topology の保存と移動
- 有限体積的な material 再構成
- persistent material element の移流

目的は、プレートを単なるセルラベルではなく、地殻の履歴を持つまとまりとして長時間更新することだった。
特に、隙間・重複・細片化・プレート連結性の崩れを、個別の後処理ではなく ownership の更新過程で観測できることを重視した。

## Decision

persistent material element 方式を唯一の実行方式として採用する。

初期プレート表面を球面三角形 element として保存し、各ステップで次を行う。

1. element をプレートの Euler 運動で移流する
2. 固定球面メッシュへ投影する
3. 発散・沈み込み・衝突に応じて material を反応させる
4. 隙間・重複・未支持領域を診断する
5. 投影結果からセルの `plate_id`、地殻種別、年齢を再構成する

influence center、Euler front、旧 surface material、有限体積再構成、共有 boundary arrangement を実行時の
選択肢としては残さない。`plate_ownership_model` の設定項目も削除し、実装上の ownership はこの方式に固定する。

## Rationale

influence field は計算量が小さいが、移動する centroidal Voronoi field へ近づき、物質の移動履歴を保持しない。
Euler front は局所的な移動を制御しやすいが、セル単位の丸め、donor floor、fragment rejection、面積 budget などの
補正が増え、プレート物質そのものを正本にできない。

共有 boundary topology は境界の連続性を検証するために有用だったが、収束時の重複や発散時の新生 material を別の
仕組みで補う必要があった。有限体積再構成は面積を扱えるが、混合セルを再び polygon へ戻す段階で隣接セルの
interface が一致せず、長時間の ownership authority にはできなかった。

persistent material element は、element の面積・位置・海洋性・年齢を持続できる。投影はサンプリングとして扱い、
境界反応を gap / overlap として記録できるため、地殻履歴とプレート所属を同じ状態から説明できる。

## Trade-off

element は球面上の区分的測地三角形であり、内部変形を解かない。衝突スタックの上下構造、マントル対流、
三次元沈み込み帯を直接表現するものでもない。新しい ridge material の生成と subduction による除去も、
メッシュ解像度と局所反応の近似である。さらに、平均メッシュセル面積の 0.01% 未満になった element は、
固定メッシュへの `f32` 投影で安定して扱えない数値的な塵として破棄する。これは極小断片の履歴を失う代わりに、
投影不能な断片による長時間更新の破綻を防ぐ近似である。

この単純化と引き換えに、長時間の物質履歴、局所的な面積収支、海洋地殻年齢、地形更新との接続を優先する。

## Consequence

方式比較用の実行分岐、不要な topology / influence / front ownership state は削除する。
比較結果と棄却理由はこの decision document に残し、現在の挙動は `docs/reference/modules/geology.md` を正本とする。

## Outcome

persistent material elementは地殻物質履歴の正本として維持する。一方、独立剛体移流したelementのgap/overlapを
排他的 `plate_id` へ直接rasterizeすると連結性を壊すことが長期seriesで判明したため、plate ownershipの正本は
[topology-preserving shared front](260822-topology-preserving-material-front.md)へ移した。
