# 非Crust期のplate更新頻度を実時間で間引く

## Status

Rejected

## Context

plate materialの移流・固定meshへのprojection・boundary reactionは、level 6では地質更新時間の大半を占める。
現行のepoch tickはCrustで500万年、Environmentで100万年、Lifeで1000年、Civilizationで100年、
Historyで1年である。一方、plate更新は全epochで毎tick実行され、短い社会・生態系tickでも全地球のplate
surfaceを再計算している。

剛体plate速度は地質学的な時間幅で平均化されたEuler運動として扱う。MORVELは主要plateの運動を地質学的
時間幅で平均したkinematic modelであり、Freyの既存速度校正もDeMets, Gordon & Argus (2010),
doi:10.1111/j.1365-246X.2009.04491.x を参照する。

## Proposal

Crust期は従来どおり毎tick、500万年ごとにplate dynamicsを実行する。非Crust期では、累積実時間が
500万年に達したときだけplate dynamicsを実行する。

| epoch | 1 tick | plate更新間隔 | 1回のplate更新へ渡す経過時間 |
| --- | ---: | ---: | ---: |
| Crust | 500万年 | 1 tick | 500万年 |
| Environment | 100万年 | 5 tick | 500万年 |
| Life | 1000年 | 5000 tick | 500万年 |
| Civilization | 100年 | 50000 tick | 500万年 |
| History | 1年 | 500万 tick | 500万年 |

間引かれたtickではplate kinematics、persistent material移流、projection、boundary reaction、
plate起源のsurface forcingを更新しない。気候、水文、生態、社会は現在のepoch tickで継続する。

重要なのは、更新を単にskipしないこと。更新実行時には累積年数をplate kinematicsとplate起源のsurface
forcingへ渡し、速度・変位・年齢増分の積分量を保存する。epoch遷移時の残余時間は次epochへ持ち越さず、
clockの境界で決定論的に切り捨てる。

## Validation

- Crustの既存seed regressionはbitwise互換を維持する。
- Environmentは5 tickごとのplate更新後に、同じ累積時間のCrust校正に対しcell crossing、plate area、
  land ratio、persistent material gap/overlapを比較する。
- Life以降は、plate更新が発生しない標準期間でplate stateが不変であり、気候・水文・社会の更新が継続する
  ことを確認する。
- long-runでは更新境界でのsurface jumpとhydrology変化を記録し、許容幅を決定文書とbenchmarkに残す。

## Trade-off

これはplate tectonicsを500万年のoperator splittingとして近似する。Environment内の短期的なplate境界変化は
解像しない。plate速度・boundary reaction・persistent material更新は非線形であり、5回の逐次更新を1回の
更新へ置換しても同じ境界形状にならない。tick 800のplate viewで不自然に入り組んだ境界が確認されたため、
この近似は採用しない。
