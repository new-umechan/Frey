# 時代とtick制御

## 目的

この文書は、時代、tick、予算、時代遷移を定義する。
ここで扱うのは時間制御であり、各モジュールの責務詳細ではない。

## Tick

tickは単なるカウンタではなく、そのtickが表す実時間の密度を持つ。

```python
@dataclass
class Tick:
    real_years: float
    scale: EpochScale
    budgets: SubsystemBudgets
```

- `real_years`
  - 1tickが表す実世界年数
- `scale`
  - 現在の時代
- `budgets`
  - 各モジュールの更新回数近似

## 時代一覧

| 時代 | 主対象 | 1tickの意味 | 重点 |
| --- | --- | --- | --- |
| 地殻形成期 | `Geology` | 500万年 | プレート運動、境界活動、海陸骨格 |
| 環境形成期 | `Climate` | 1万年 | 降水、流量、侵食、堆積 |
| 生命誕生期 | `Ecology` | 1000年 | 可住性、生産性、土地利用ポテンシャル |
| 文明成立期 | `Civilization` | 100年 | 定住、農業、初期国家形成 |
| 歴史展開期 | `Civilization` | 1年 | 国家、文化、技術、戦争 |

時代は、モジュールを開始停止する排他的な段階ではない。
どのモジュールをどれだけ強く更新するかを決める時間スケールである。

## 状態の有効化タイミング

時代に応じて、必要な状態群を順次有効化する。

| 時代 | 有効な状態 |
| --- | --- |
| 地殻形成期 | `Geology` のみ |
| 環境形成期 | `Climate` を初回有効化 |
| 生命誕生期 | `Ecology` を初回有効化 |
| 文明成立期 | `Civilization` を初回有効化 |
| 歴史展開期 | 既存状態をすべて保持したまま運用 |

地殻形成期では、`Climate` と `Ecology` がまだ有効でなくても `Geology` が未初期化値を読まないようにする。
この時期の既定入力は次の通り。

- 降水は、地殻形成期向けの簡易な初期降水分布を使う
- 流量は 0 とする
- 流域植生は なし とする

## 予算配分

`SubsystemBudgets` は、各モジュールに与える内部更新回数の近似である。
初版では整数回数として扱う。

| 時代 | `Geology` | `Climate` | `Ecology` | `Civilization` |
| --- | --- | --- | --- | --- |
| 地殻形成期 | 高 | 低 | なし | なし |
| 環境形成期 | 中 | 高 | 低 | なし |
| 生命誕生期 | 低 | 中 | 高 | 低 |
| 文明成立期 | 低 | 低 | 中 | 高 |
| 歴史展開期 | 低 | 低 | 低 | 高 |

歴史展開期のように `Civilization` 以外の活動量が低い時代では、低活動モジュールはスキップ可能とする。
スキップ条件の閾値は後続バージョンで定義する。

## 時代遷移

時代遷移は固定tick数ではなく、状態条件で決める。
これにより、惑星ごとに時代の長さが変わる。

```python
EPOCH_TRANSITIONS = {
    GEOLOGICAL: lambda s: s.sea_land_ratio_stable(),
    CLIMATE:    lambda s: s.river_network_formed(),
    ECOLOGY:    lambda s: s.habitable_area() > THRESHOLD,
    SOCIETY:    lambda s: s.has_settlement(),
}
```

上の擬似コードは概念を示したものであり、実際の閾値や判定式は未確定である。

## 更新ループ

1tickの標準順序は次の通り。

1. tick開始時に `FeedbackQueue` の内容を `World State` に適用する
2. `Geology`
3. `Climate`
4. `Ecology`
5. `Civilization`
6. `Civilization` が環境影響を `FeedbackQueue` に格納する
7. 時代遷移判定

補足:

- 同一tick内の依存はDAGで保証する
- `FeedbackQueue` への格納は tick N の末尾で行う
- `FeedbackQueue` の適用は tick N+1 の開始時に行う
- 同一tick内で逆向きの即時反映は行わない

## 将来の拡張候補

- モジュールごとの内部時間幅
- 計算コスト上限
- 活動量に応じたスキップ率
- 海流、火山など複数スケール現象の扱い
