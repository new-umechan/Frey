# Plate motion terrain response verification

## Status

Accepted

## Context

Frey は plate の相対運動を法線・接線成分へ分解し、境界分類と地形更新へ反映する。
従来のテストは速度成分と分類を確認していたが、地形変化の符号、片側性、局在を
更新経路全体では確認していなかった。現在の ETOPO 標高は長期積分された状態なので、
1 tick の `delta height` とは直接比較しない。

## Decision

実地球データとの比較より先に、人工境界で次を分離して verification する。

1. 相対速度の法線・接線分解
2. 収束、発散、横ずれの分類と subducting / overriding side
3. 境界条件から地形変化を作る forcing

境界 edge の端点を `a`, `b`、法線を `a` から `b` とすると、規約は次とする。

```text
relative_velocity = velocity_b - velocity_a
relative_normal_velocity = dot(relative_velocity, boundary_normal)
convergence = max(0, -relative_normal_velocity)
divergence = max(0, relative_normal_velocity)
```

法線速度の正は発散、負は収束である。純横ずれは収束・発散固有の forcing を作らない。
沈み込みでは trench を subducting side、arc uplift と volcanism を overriding side へ置く。
対称な発散入力では ridge 応答も対称にする。

帯状 fixture で片側性、符号、速度依存性、鏡映を検査し、球面 fixture で端点交換と
全球回転に対する不変性を検査する。forcing 計算は内部関数へ分離し、本番更新とテストで共有する。

## Consequences

この verification は更新則の向きと符号を保証するが、地球の絶対標高、海溝深度、山地幅、
長期地形史の妥当性は保証しない。それらは長期積分と観測データを用いる validation で扱う。
時間刻み依存性、mutation の一括実行、成熟した沈み込み帯との比較は別の検証拡張とする。
