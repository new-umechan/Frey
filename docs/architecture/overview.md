# Architecture

## 設計思想

歴史シミュレータの設計軸は、「何を扱うか」ではなく「状態の変化速度」とする。

遅いものは速いものの条件を決める。
速いものは遅いものを少しずつ侵食する。
この非対称な双方向関係を、個別の特例ロジックではなく、コード構造そのものに埋め込む。

この方針により、文明が環境を破壊して自滅する、気候変動が国家を崩す、といった現象を自然に表現する。

## 全体構成

トップレベルモジュールは次の5つに固定する。

```text
Exec
├── Geology
├── Climate
├── Ecology
└── Civilization
```

- `Exec`
  - tick進行、予算配分、時代遷移、履歴、フィードバック適用を担当する
- `Geology`
  - 地形の変化が遅く蓄積する領域を担当する
- `Climate`
  - 気候と水循環のような中間速度の環境変化を担当する
- `Ecology`
  - 生態と可住性のような環境応答を担当する
- `Civilization`
  - 人口、国家、技術など比較的速い社会変化を担当する

## モジュール間通信の原則

モジュール間の直接依存は持たない。
すべてのモジュールは共有の `World State` を読み書きする。
更新器はステートレスに保つ。

```text
┌─────────────────────────────┐
│         World State          │
│  各セルが持つ属性の現在値    │
│ （標高・降水・植生・人口…）  │
└─────────────────────────────┘
         ↑読み書き↑
┌──────┬──────┬──────┬──────┐
│Geo   │Cli   │Eco   │Civ   │
│更新器│更新器│更新器│更新器│
└──────┴──────┴──────┴──────┘
```

ここでいう `World State` は、各セルが持つ現在値の集合である。
各モジュールは他モジュールの内部実装を知らず、`World State` だけを共有面として使う。

## 更新順序

同一tick内の依存はDAGで表し、順序を保証する。
同一tick内で循環依存は作らない。
フィードバックは次tick以降に遅延させる。

```python
UPDATE_DAG = {
    Geology:      [],
    Climate:      [Geology],
    Ecology:      [Geology, Climate],
    Civilization: [Geology, Climate, Ecology],
}

FEEDBACK_EDGES = {
    Civilization: [Geology, Climate, Ecology],
}
```

意味は次の通り。

- `Geology` は最初に走る
- `Climate` は地形条件を読んで更新する
- `Ecology` は地形と気候を読んで更新する
- `Civilization` は地形、気候、生態を読んで更新する
- `Civilization` が環境へ与える影響は、その場で逆流させず、次tick用に遅延させる

## 河川の扱い

河川は複数の時間スケールにまたがるが、帰属は `Geology` に固定する。

原則は単純である。
地形を書き換える責任は `Geology` が持つ。

河川に関する責務分担は次の通り。

| モジュール | 担当 |
| --- | --- |
| `Geology` | 流路の決定、侵食、堆積、地形書き換え |
| `Climate` | 降水量、流量を `World State` へ書く |
| `Ecology` | 流域植生との交換を `World State` へ書く |
| `Civilization` | 取水、ダム、汚染を `World State` へ書く |

河川という独立モジュールは作らない。
河川は `World State` 上にある属性群として扱う。

流路計算は、標高を読んで流路グラフを返す純粋関数として切り出す。

## 関連文書

- 時代、tick、予算、遷移
  - `docs/architecture/phase_control.md`
- `World State` と `Exec` 側の状態配置
  - `docs/architecture/data_model.md`
- 各モジュールが何を読み、何を書くか
  - `docs/architecture/module_boundaries.md`
