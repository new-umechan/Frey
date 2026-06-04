# 初期プレート分割を球面べき乗ボロノイへ単純化する

## Status

Accepted

## Context

`docs/research/search.md` は、最初のプレート形状生成について既存研究がないため、
「マントルからプレートのかたちを生成する」MVP として球面べき乗ボロノイを採用する方針を示している。

一方、現行実装の初期プレート分割は、seed からメッシュ近傍へ多源伝播する経路コスト最小化になっている。
この経路コストには、`phi`、境界帯、プレートごとの anisotropy、warp basis、roughness、edge noise が混在している。

そのため、学術的に正しいものではないにも関わらず、「それらしさ」を出すための複雑性が
混じっている状況になり、あとから差し替えることも難しくなっている。

## Decision

初期プレート分割は、球面上の additively weighted Voronoi、つまり球面べき乗分割として定義する。

各セル位置を単位球面上の点 `x`、プレート seed 位置を `s_i`、プレート重みを `w_i` とし、
セル `x` の `plate_id` は次で決める。

```text
plate_id(x) = argmin_i ( d_sphere(x, s_i)^2 - w_i )
d_sphere(x, s_i) = acos(clamp(dot(x, s_i), -1, 1))
```

同点時は小さい `plate_id` を採用する。

実装方針:

- `plate_count` は既存 params の `plate_count_min` / `plate_count_max` から決める。
  ただし、低解像度メッシュでは非空・連結の初期 plate を作れるように、
  生成前にメッシュセル数から有効上限をかける。
- seed 抽出は `phi` の局所極大/極小候補を使わず、farthest-point sampling へ寄せる。
- farthest-point sampling は固定方向に最も近いセルから開始し、以後は既存 seed への最小球面距離が最大のセルを追加する。
- farthest-point sampling の同距離 tie-break は cell index で決定的に解決する。
- プレート重み `w_i` は `world_seed` から決定的に派生した乱数列で生成し、平均 0 に正規化する。
- `w_i` の単位は `d_sphere^2` と同じ radian^2 とする。
- `w_i` は bounded uniform で生成し、標準偏差が `0.20 * target_angle^2` になるようにする。
- `target_angle = sqrt(4π / plate_count)` とする。
- 分割時はメッシュ辺の経路コスト、anisotropy、warp basis、edge noise、boundary band penalty を使わない。
- 空 plate が出た場合は生成失敗として扱い、重み幅の自動縮小や生成後の `plate_count` の compact はしない。
- 連結性後処理は原則不要とする。
- 1つでも非連結 fragment が出た場合は生成失敗として扱う。
- `phi` は seed 配置と分割境界を決める用途から外し、採用する場合もプレート属性や初期標高骨格の入力に限定する。

## Rationale

球面べき乗分割なら、初期プレート境界の定義がセルごとの独立した最小化問題になる。
これは `docs/research/search.md` の「正確性は棚上げしつつ、さかのぼれる体験を作る」MVP と相性がよい。

現時点のマントル場は信頼性が低いため、seed 配置を `phi` に依存させると、未検証の場を初期プレート数・位置へ直接反映してしまう。
初期 plate seed は farthest-point sampling で球面上に安定して配置し、プレート面積のばらつきやランダム性は `world_seed` 由来の `w_i` へ集約する。
Poisson disk は採用しない。
seed 間隔と重みの両方でランダム性を管理すると、面積分布の原因が二重化し、調整と検証が難しくなるためである。

この決定は `docs/research/search.md` の球面べき乗ボロノイ方針を採用するが、MVP では mantle-derived shape ではなく mantle-independent initialization として扱う。
マントル場は、信頼性が上がるまで初期プレート配置の正本入力にしない。

現行の多源伝播は、見た目の多様性を作れる一方で、べき乗ボロノイではなく、速度場や境界形成史でもない。
この段階では複雑な歪みを初期分割へ入れるより、後続のプレート運動、剛体回転、境界活動、地形応答で因果を作る方が説明しやすい。

重み付き分割にすると、seed だけの通常 Voronoi よりプレート面積のばらつきを表現できる。
ただし、そのばらつきは `w_i` として明示されるため、後から分布や上限を検証しやすい。
`0.20 * target_angle^2` は均等配置を目的にした値ではなく、seed 配置側からランダム性を抜きつつ、
視覚的に確認できる面積ばらつきを `w_i` へ持たせるための基準 scale である。
bounded uniform は極端な tail で seed 自身のセルが奪われることを避けるための実装上の安定化である。

## Consequences

利点:

- 初期分割の式と実装が一致する
- `partition_plates` が単純化され、挙動の検証がしやすくなる
- 初期プレート形状と後続 dynamics の責務境界が明確になる
- `phi` の役割が属性 / 標高骨格に整理され、初期プレート配置から切り離される

欠点:

- 現行の warp / anisotropy 由来の有機的な境界形状は失われる
- seed と重みの設定次第では、境界が単調で人工的に見える可能性がある
- 離散メッシュ上では、理論上の連続球面分割と完全には一致しない
- 空 plate や非連結 fragment を生成失敗にするため、実装時には failure path と診断ログが必要になる

## Implementation Notes

実装に入る場合の主な変更対象:

- `rust/src/sim/geology/plates.rs`
    - `partition_plates` を球面べき乗距離の直接評価へ置き換える
    - `PlatePartitionInput` から分割に不要な `phi`、`plate_cost_warp_basis`、`nbrs`、`boundary_band` 依存を外す
    - `build_plate_growth_profiles`、`generate_plate_cost_warp_basis`、`sample_plate_warp_mid`、`edge_noise_signed` などを削除または後続用途へ限定する
- `docs/reference/modules/geology.md`
    - decision 採用後に、6.3 の「従来仕様のアルゴリズムを基本維持する」を置き換える

検証観点:

- 全セルの `plate_id` が有効範囲内にある
- 各プレートが少なくとも 1 セルを持つ
- 各プレートが単一連結成分である
- seed 固定時に分割が決定的である
- `world_seed` 固定時に `w_i` と分割が決定的である
- 通常 Voronoi と重み付き Voronoi の面積分布を比較できる

## Unresolved

- bounded uniform と `0.20 * target_angle^2` の組み合わせが十分な seed 範囲で安定するか。
  通常 Voronoi と重み付き Voronoi の面積分布・見た目を継続比較して調整する。
- 生成失敗時に、UI/API へどの粒度で診断を返すか。
- 将来、信頼できるマントル場ができた場合に `w_i` へ mantle bias を足すか。
  初期実装では `phi` を `w_i` に反映しない。
