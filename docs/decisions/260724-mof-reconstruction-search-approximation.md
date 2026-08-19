# MoF界面探索の近似による事前計算短縮

## Status

Rejected

## Context

level 6 の persistent material 更新では、海嶺セルの Moment-of-Fluid (MoF) 再構成が支配的な
計算の一つである。MoF はセルごとの面積（零次モーメント）を保存し、一次モーメントの欠損を
最小化して界面を求める手法である。Dyadechko and Shashkov (2008) は、面積保存を保ちながら
一次モーメント欠損を最小化する多材料MoF再構成を記述している。

現在の実装は界面方向を16方向で走査し、その後8回の三分探索を行う。これは視覚的なプレート
境界と地殻面積収支に対して必要以上の方向精度であり、完全一致したframe出力を優先するより、
計算量と見た目・診断値の品質を両立させる方針へ移行する。

## Proposal

界面方向探索を8方向、三分探索を4回に変更する案を評価した。切断面の位置は従来どおり面積比から
求めるため、各material phaseの面積は保存するが、与えられた一次モーメントへ近づける界面方向に
近似誤差を導入する。

採用条件は次のすべてを満たすこととする。

1. persistent material の単体テスト、および既存の面積・coverage診断が通ること。
2. `alpha` を含むseed regressionの許容偏差内であり、fatalな未投影elementが発生しないこと。
3. level 6 のplate viewとterrain viewで、海嶺・海溝・大陸境界に明らかな格子状ノイズ、穴、
   不連続がないことを目視確認すること。
4. level 6 のCrust期10tickで、MoF再構成時間が基準実装より十分に短縮されること。

## Scientific basis

MoFは、materialのvolumeとcentroid（零次・一次モーメント）から界面を再構成する有限体積手法である。
本変更はその保存量を変更せず、一次モーメント欠損最小化の数値探索精度を下げる実装上の近似である。

- Dyadechko, V. and Shashkov, M. (2008), *Reconstruction of multi-material interfaces from
  moment data*, Journal of Computational Physics 227(11), 5361–5384,
  [doi:10.1016/j.jcp.2007.12.029](https://doi.org/10.1016/j.jcp.2007.12.029).

## Trade-off

局所的な界面の向きと一次モーメントは粗くなるため、frameのバイト完全一致は保証しない。一方、
面積保存、material種別、年齢、既存のgap/overlap診断を品質ゲートとして維持する。採用後の
visual regressionで問題が出た場合は、探索回数を段階的に戻すか、この決定をRejectedにする。

## Rejected alternative

triangleの球面重心を周囲3cellへ面積重みで分配するmass-lumped投影も試したが、identity projection、
剛体回転coverage、境界bandへの誤差局在という既存テストを満たさなかった。特に剛体回転後の
uncovered areaが `0.0429` まで増え、誤差が境界外にも広がったため採用しない。

reaction後の全域再投影を次のplate更新まで遅延する案は、自動テストと短いseed regressionを通過したが、
plate viewの目視でプレート境界が不自然に入り組んだ。persistent elementの反応結果と表示用projectionを
同じ更新内で同期する必要があるため、この案も採用しない。

MoF探索を8方向・4回へ削減する案も、同じ目視確認で計算量削減前から明確に乖離した不自然な境界を
生んだため採用しない。自動テストと短いseed regressionだけでは、この境界形状の劣化を検出できなかった。
