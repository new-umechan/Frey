# Relative plate motion terrain response

## Status

Accepted

## Context

`docs/research/search.md` では、マントル対流から厳密にプレートと地形を解くのは
研究規模になりやすいため、MVP では因果を辿れる近似を優先するとしている。
既存の Geology は damage-first plate emergence、plate velocity、
境界分類、surface dynamics を持っているが、初期地形生成と runtime 更新で
境界運動 proxy の意味が完全には揃っていなかった。

参考にする代表的な事実は次である。

- プレート速度はおおむね cm/yr、Frey の Crust tick では数十 km/Myr の代表値で読む
- 古く高密度な海洋地殻ほど沈み込みやすく、slab pull / rollback の proxy になる
- 収束境界は衝突山脈、海溝、火山弧、背弧を作る
- 発散境界は海嶺またはリフトを作る
- 横ずれ境界は狭い relief と shear stress を作る

## Decision

境界 edge ごとに plate 相対速度を法線・接線成分へ分解し、
初期生成と runtime の両方で同じ意味の proxy として使う。

```text
rel_v = v_b - v_a
n = unit(pos_b - pos_a)

C = max(0, dot(rel_v, n))
D = max(0, -dot(rel_v, n))
T = length(rel_v - dot(rel_v, n) * n)
obliquity = T / (C + D + T + eps)
```

初期生成では `BoundaryEdge` に `convergence`、`divergence`、`transform`、
`obliquity`、`strength` を保持し、`apply_boundary_model` はこれらから
海溝・弧・背弧・衝突・リフト・海嶺・transform relief を合成する。

runtime では `BoundaryDynamicsState` に同じ proxy をセルごとに保持し、
`surface_dynamics` の stress、tectonic uplift、tectonic subsidence、
volcanism へ反映する。

大陸衝突山脈は境界線上の細いピークではなく、縫合線近傍の弱い notch、
境界から少し離れた core uplift、広い plateau 成分の合成として扱う。
これにより、衝突境界が単一セル幅の鋭い壁になることを避ける。

沈み込み帯の火山弧は海溝から内陸側へずらし、ずれ量は slab dip と
火山発生深度の proxy から決める。

```text
dip = lerp(25deg, 65deg, subduction_angle_proxy)
target_depth = lerp(90km, 130km, subduction_gate)
arc_distance_from_trench = target_depth / tan(dip)
```

急角度の沈み込みでは弧は海溝に近く、緩角度の沈み込みでは遠くなる。

沈み込み適性は、海洋地殻の年齢 proxy、密度 proxy、収束履歴、現在の収束成分から作る。

```text
subduction_gate =
    0.45 * age_norm
  + 0.30 * density_age_factor
  + 0.15 * convergence_memory
  + 0.10 * convergence_norm
```

この式は厳密な force balance ではなく、coarse mesh で因果が読める地形を作るための
手続き的 proxy とする。

## Consequences

利点:

- 初期生成と runtime の境界応答を同じ語彙で説明できる
- 収束・発散・横ずれの地形差が `BoundaryType` だけでなく相対運動量にも依存する
- 既存 config schema を増やさず、既存 gain / width の較正を維持できる

欠点:

- `convergence` / `divergence` / `transform` は coarse mesh 上の相対速度 proxy であり、
  実際の応力場や rheology を解くものではない
- 沈み込み角、弧位置、背弧沈降は経験的な band / ring response であり、
  局所的な地質差までは表現しない
- runtime の rollback / convergence memory は履歴 proxy なので、
  長期の slab evolution を保存則つきで追跡するものではない
