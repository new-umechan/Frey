# プレートに基づく地形生成仕様

## 1. 目的

入力seedとparamsから、プレート運動に整合する初期地形を生成する。
出力は少なくともheight、plate_id、river_flux、river_nextを含む。

## 2. 入出力

### 2.1 入力

- seed: String
- params: TerrainParams

```rust
pub struct TerrainParams {
    pub level: u32,                   // icosphere分割レベル（推奨 6）
    pub harmonic_max_l: u32,                   // 球面調和次数（既定 4）
    pub spectral_alpha: f32,                   // スペクトル減衰（既定 1.5）
    pub plate_count_min: u32,          // 最小プレート数（既定 8）
    pub plate_count_max: u32,          // 最大プレート数（既定 18）
    pub ocean_plate_ratio: f32,       // 海洋プレート比率（既定 0.65）
    pub boundary_band: f32,           // 境界帯域の閾値（既定 0.08）
    pub boundary_convergent_base_gain: f32, // 収束境界の基礎地形係数
    pub boundary_divergent_base_gain: f32,  // 発散境界の基礎地形係数
    pub boundary_transform_relief_gain: f32,// 横ずれ境界の微地形係数
    pub trench_gain: f32,             // 海溝の強さ
    pub arc_gain: f32,                // 火山弧/島弧の強さ
    pub collision_gain: f32,          // 大陸衝突帯の造山強度
    pub rift_gain: f32,               // リフト沈降の強さ
    pub boundary_trench_width: f32,   // 海溝帯の幅
    pub boundary_arc_width: f32,      // 弧状隆起帯のオフセット幅
    pub boundary_collision_width: f32,// 衝突造山帯の幅
    pub boundary_rift_width: f32,     // リフト帯の幅
    pub boundary_obliquity_mix: f32,  // 斜交収束/発散の緩和係数
    pub boundary_distance_falloff: f32, // 境界距離減衰
    pub boundary_anisotropy: f32,     // 境界帯の異方性
    pub river_rain_base: f32,         // 降水ベース（既定 0.5）
    pub river_accumulation_threshold: f32,   // 河川成立閾値（既定 0.015）
    pub erosion_iterations: u32,            // 水食侵食反復（既定 12）
    pub hydraulic_erosion_rate: f32,    // 侵食率（既定 0.020）
    pub hydraulic_deposit_rate: f32,  // 堆積率（既定 0.35）
    pub sediment_capacity_gain: f32,  // 土砂容量係数（既定 0.90）
    pub erosion_min_slope: f32,       // 最小勾配（既定 0.002）
    pub erosion_max_delta_per_iter: f32, // 1反復の最大変化量（既定 0.015）
    pub coastal_deposit_rate: f32,    // 沿岸・浅海堆積率（既定 0.45）
    pub shallow_sea_floor: f32,       // 浅海閾値（既定 -0.08）
}
```

JavaScript側から呼ぶ場合は、paramsの全フィールドを渡す。

### 2.2 出力

```rust
pub struct TerrainOutput {
    pub height: Vec<f32>,         // len = V, [-1, 1]
    pub plate_id: Vec<u32>,       // len = V, [0, num_plates)
    pub river_flux: Vec<f32>,     // len = V, [0, 1]
    pub river_next: Vec<i32>,     // len = V, -1 は海/終端
}
```

seedがearthの場合は生成を行わず、プリセット地形を返す。

## 3. 決定性ルール

- 乱数生成器は固定アルゴリズムの疑似乱数を使う
- 初期化シードはseedとparamsを正規化した文字列から計算する
- 侵食パラメータもparams正規化文字列へ含める
- 同一入力でplate_id一致、heightは実用上同一の再現性を目標とする

## 4. 生成手順

### 4.1 メッシュ準備

1. generate_icosphere(level)でbase_posとtri_indicesを取得
2. tri_indicesから無向辺を列挙し、近傍CSRを構築
3. 各頂点の球面座標を計算

### 4.2 マントル場 φ の構築

各頂点のphiを次で評価する。

```text
phi(theta, lambda) = Σ_{l=2..L_max} Σ_{m=-l..l} c_lm * Y_lm(theta, lambda)
c_lm ~ Normal(0, sigma_l), sigma_l = 1 / l^spectral_alpha
```

評価後にz-score正規化する。

### 4.3 プレートseed抽出

1. phiの局所極大・極小を検出
2. 極大と極小から必要数を採用するが、seed同士の最小距離制約を課す
3. 制約が厳し過ぎて不足する場合は段階的に距離閾値を緩和する
4. 極値不足時はfarthest-pointで補完（同様に距離制約を優先）
5. 最後に不足が残る場合のみ乱数で補完
6. プレート数はplate_count_minからplate_count_maxの範囲でseed依存に決定

意図:
- seed近接による「小さいプレートが大きいプレート内に発生しやすい」状態を減らす
- プレートサイズの極端な偏りを抑える

### 4.4 プレート分割

1. 各seedから多源伝播でplate_idを決定
2. 伝播コストは辺長、境界ペナルティ、plateごとの拡張係数で決める
3. さらに各plateに「成長しやすい向き（preferred growth axis）」と異方性強度を持たせる
4. 辺方向が好みの向きに沿うほどコストを下げ、直交するほどコストを上げる
5. エッジ単位の決定的乱数ゆらぎを加える
6. 頂点上の空間相関ノイズ場（低周波+中周波）で伝播コストを歪める
7. phiが高い領域ほど伝播コストを割引する
8. 未到達頂点は最短seedに再割当
9. 分割後、連結成分ベースの飛び地/入れ子除去を行う
10. 小さな飛び地成分、または単一プレートに完全に囲まれた成分を隣接プレートへ吸収する（反復）

方向依存コストの意図:
- プレート境界を単純なVoronoi状から崩し、細長い/方向性のあるプレート形状を作る
- 異方性はプレートごとにランダムに与えるが、seedとparamsに対して決定的である
- 空間相関ノイズにより、局所的なギザギザではなく大局的に歪んだ境界を作る
- 飛び地/入れ子除去により、プレートの連結性と見た目の自然さを改善する

概念式（簡略）:

```text
step_cost = edge_len * (1 + boundary_penalty) / spread
direction_factor = 1 + anisotropy * (1 - |dot(edge_dir, tangent(preferred_axis))|)
phi_factor = clamp(1 - k_phi * phi_mid, min_phi_factor, max_phi_factor)
warp_factor = clamp(1 + k_warp * warp_mid, min_warp_factor, max_warp_factor)
random_factor = 1 + roughness * edge_noise(edge, plate)
next_cost = prev_cost + step_cost * direction_factor * phi_factor * warp_factor * random_factor
```

ここで `tangent(preferred_axis)` は現在頂点の接平面へ射影した向き。
`warp_mid` は隣接2頂点の空間相関ノイズ場の平均値。

### 4.5 プレート属性付与

各plateに海洋/大陸フラグ、速度、基準標高を設定する。

- 海洋プレート基準標高は浅めに設定
- 大陸プレート基準標高は高くなり過ぎない範囲で設定

### 4.6 初期標高

高さは基準標高、phi、微小ノイズを合成して作る。
途中段階では広い範囲に制限し、後段で再調整する。

### 4.7 境界相互作用

境界辺を抽出して収束・発散・横ずれを判定し、境界距離場に基づく帯状の地形特徴を生成する。

タイプ分類:
- v_rel_n > +eps: 収束
- v_rel_n < -eps: 発散
- それ以外: 横ずれ

収束境界では、海洋-大陸/海洋-海洋/大陸-大陸を分けて扱う。
- 海洋-大陸: 海溝 + 前弧 + 火山弧帯
- 海洋-海洋: 海溝 + 島弧帯
- 大陸-大陸: 幅広い造山帯

発散境界ではリフト帯（沈降帯）を生成する。
横ずれ境界では大規模な高低差は抑え、微地形的な粗さのみを付与する。

境界影響は固定 hop 拡散ではなく、最近傍境界への距離（弦長近似）で減衰させる。

### 4.8 水食侵食（簡易）

境界補正・褶曲帯適用後の地形に対し、球面メッシュ上のセル型水食侵食を適用する。

1. 各反復で暫定河川（river_next, river_flux）を再計算する
2. 陸セル（height > 0）のみを侵食対象にする
3. 流量と勾配から侵食量を計算し、1反復の変化量に上限をかける
4. 下流の平坦化や終端で堆積を発生させる
5. 海への流出は基本的に系外へ捨てるが、沿岸・浅海セルには減衰付きで堆積を許可する
6. 更新はバッファ方式で同時反映する

湖・内陸盆地の溢流や海底全体の侵食は、この段階では扱わない。

### 4.9 海面再調整と後処理

固定sea_levelではなく、分位点から海面を再推定して全体の海陸バランスを調整する。
その後、海岸付近と低標高域に抑制をかけ、海岸線の山脈化や標高差の過剰を防ぐ。

### 4.10 河川生成

1. 緯度依存の降水を計算
2. 最急降下先をriver_nextとして設定
3. 高地から低地順でriver_fluxを集水
4. 海セルのriver_nextは-1

## 5. 描画時の扱い

地形計算では海溝などの海面下地形を保持する。
ただし現在の描画では、海面下の頂点変位は半径へ反映しない。
見た目上の海の亀裂を防ぐため、地形変位は陸地のみを対象にする。

## 6. バリデーション基準

### 6.1 形状/範囲

- 各配列長が頂点数と一致
- heightは-1から1の範囲
- plate_idは有効範囲内
- river_nextは-1または有効頂点番号

### 6.2 統計

- 海セル比率はおおむね 0.55 から 0.80
- プレート数は指定範囲内

### 6.3 決定性

- 同一seed + paramsでplate_idが一致
- heightの再現性が維持される

## 7. 失敗時の扱い

- seed抽出不足時はfarthest-point補完
- 空プレートが出る場合は再ラベル
- 河川ループ検出時は勾配の弱い辺を切って終端化

## 8. 計算量目安

分割レベルL=6のとき、頂点数は40962、辺数はおおむね3V程度になる。

主要工程の計算量は次の通り。

- 球面調和評価はO(V * L_max^2)
- プレート分割の多源伝播はO(E log V)
- 境界補正の減衰拡散はO(E)
- 水食侵食は各反復で河川再計算を含み、おおむねO(erosion_iterations * (V log V + E))
- 河川集水は高さソートを含みO(V log V)

L_max、erosion_iterationsを固定した運用では、全体の支配項はおおむねV log Vになる。
