# 高次の設計調査メモ

更新日: 260529

論文を読み、学術的なただしさをある程度保証するためのメモ。
物理的な厳密性よりも、因果関係を辿れる分野の幅広さを重視する。

現実の地球から生成することは難しい
初期状態を、恣意性を排除しながらどう設計できるか？

MINISFORUM MS-A2 Ryzen 9 9955HXのメモリ96GBをサーバーとして使えるから、そこで十分動かせる感じで。
科学的正確性はある程度意識しながらも、時間発展する様子をビジュアライズしたい

## 分野自体への理解

[The generation of plate tectonics from mantle convection](https://www.sciencedirect.com/science/article/abs/pii/S0012821X02010099)
研究のまとめ的な文献

## マントルの動き生成

ストークス方程式を解きたくない（速度がほとんどないため、慣性項を無視できるからナビエ・ストークス方程式ではない）
完全な全休3D
初期状態の生成はどう行うか？
ゆくゆくはサロゲートモデルで実装かな？

物理的なレイヤーに加えて、描画用にトレーサー粒子をくわえる

> マントル対流がプレート運動と単純に対応しているとは考えられていません

https://www-old.eps.s.u-tokyo.ac.jp/epphys/solid/mantle.htmlより

マントルの対流の仕組み自体よくわかっていなさそう

## マントルからプレートのかたちを生成する

既存研究がなさそうなため、MVPとして
球面べき乗ボロノイで作成

lid regimeの区別について:
[Dissecting the puzzle of tectonic lid regimes in terrestrial planets](https://www.nature.com/articles/s41467-025-65943-1)

一旦さかのぼれる体験を作れることを優先し、正確性は棚上げしておく。

(260605追記)
自己生成するプレートのモデルはあった:
[プレート生成についての既存研究のまとめ](https://earth.yale.edu/sites/default/files/2024-08/Bercovici%20doc%202.pdf)

## dynamic topography

一旦おいておいてもよい

## プレートの動き

駆動力としては、slab pull、ridge push、mantle drag / basal dragがある
`docs/research/topography_climate.md`より

最初は、マントル由来の力の場を作り、プレートを剛体として動かすところから始まる？
力のモーメントの釣り合いから

### 初期化

初期化について
subduction initiation model

沈み込み開始からプレート運動を作るモデル

## プレートの動きから地形の出力

Procedural Tectonic Planets: `docs/research/procedural_tctonic_planets.md`
手で編集できる、それらしいものを作ることを優先しているらしい。
プレートがどのように動くと、どのような地形を侵食作用まで

goSPL : `docs/research/gospl.md`
地形の出力を目指していたりもする

## プレートの分裂、合体

## プレートをMVPとしてどう実装するか？

完璧なものを作ろうとすれば、それは研究レベルになってしまう。
しかし、MVPとして実装するのも恣意性が高い
260605時点でできているところから、うまくいっていないところをパッチ的に修正していく対応しか、
結局できないのかもしれない

## 河川による侵食

河川

- goSPL
浸透は考慮されていない

時間経過でSFDからMFDへ
湖は、埋まったところを解決するように

## 地層

## 氷河

## 大気構成

はたしてセルごとに保持する必要があるのか？
