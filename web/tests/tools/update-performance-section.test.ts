import { describe, expect, it } from "vitest";

import {
  arePerformanceValuesEqual,
  buildComparisonRowsWithFallback,
  parsePerformanceValues,
} from "../../../tools/todo/update-performance-section";

describe("update-performance-section", () => {
  it("parses the current-value column from the reversed comparison table", () => {
    const section = [
      "| 指標 | 前回値 | 現在値 | 差分 | 単位 | 最終更新 |",
      "| --- | ---: | ---: | ---: | --- | --- |",
      "| 初期読み込み時間 | 25.101 | 20.734 | -4.367 | ms | 2026-04-19 23:36 |",
      "| WASM バンドルサイズ | 2694.49 | 2715.64 | +21.15 | KiB | 2026-04-19 23:36 |",
    ].join("\n");

    expect(parsePerformanceValues(section)).toEqual([
      {
        label: "初期読み込み時間",
        value: "20.734",
        unit: "ms",
        updatedAt: "2026-04-19 23:36",
      },
      {
        label: "WASM バンドルサイズ",
        value: "2715.64",
        unit: "KiB",
        updatedAt: "2026-04-19 23:36",
      },
    ]);
  });

  it("parses values even when comparison columns are swapped", () => {
    const section = [
      "| 指標 | 現在値 | 前回値 | 差分 | 単位 | 最終更新 |",
      "| --- | ---: | ---: | ---: | --- | --- |",
      "| 初期読み込み時間 | 20.734 | 25.101 | -4.367 | ms | 2026-04-19 23:36 |",
    ].join("\n");

    expect(parsePerformanceValues(section)).toEqual([
      {
        label: "初期読み込み時間",
        value: "20.734",
        unit: "ms",
        updatedAt: "2026-04-19 23:36",
      },
    ]);
  });

  it("ignores updatedAt when comparing values", () => {
    const left = parsePerformanceValues(
      [
        "| 指標 | 前回値 | 現在値 | 差分 | 単位 | 最終更新 |",
        "| --- | ---: | ---: | ---: | --- | --- |",
        "| 初期読み込み時間 | 10.000 | 20.734 | +10.734 | ms | 2026-04-19 23:36 |",
      ].join("\n"),
    );
    const right = parsePerformanceValues(
      [
        "| 指標 | 前回値 | 現在値 | 差分 | 単位 | 最終更新 |",
        "| --- | ---: | ---: | ---: | --- | --- |",
        "| 初期読み込み時間 | 10.000 | 20.734 | +10.734 | ms | 2026-04-19 23:47 |",
      ].join("\n"),
    );

    expect(arePerformanceValuesEqual(left, right)).toBe(true);
  });

  it("walks history until finding a different snapshot value", () => {
    const current = [
      {
        label: "初期読み込み時間",
        value: "21.090",
        unit: "ms",
        updatedAt: "2026-04-19 23:44",
      },
    ];
    const previous = [
      {
        label: "初期読み込み時間",
        value: "21.090",
        unit: "ms",
        updatedAt: "2026-04-19 23:44",
      },
    ];
    const history = [
      [
        {
          label: "初期読み込み時間",
          value: "21.090",
          unit: "ms",
          updatedAt: "2026-04-19 22:10",
        },
      ],
      [
        {
          label: "初期読み込み時間",
          value: "25.101",
          unit: "ms",
          updatedAt: "2026-04-19 16:33",
        },
      ],
    ];

    expect(buildComparisonRowsWithFallback(current, previous, history)).toEqual(
      [
        {
          label: "初期読み込み時間",
          value: "21.090",
          unit: "ms",
          updatedAt: "2026-04-19 23:44",
          previousValue: "25.101",
          delta: "-4.011",
        },
      ],
    );
  });

  it("keeps zero delta when fallback snapshot is unavailable", () => {
    const current = [
      {
        label: "初期読み込み時間",
        value: "21.090",
        unit: "ms",
        updatedAt: "2026-04-19 23:44",
      },
    ];
    const previous = [
      {
        label: "初期読み込み時間",
        value: "21.090",
        unit: "ms",
        updatedAt: "2026-04-19 23:44",
      },
    ];

    expect(buildComparisonRowsWithFallback(current, previous, [])).toEqual([
      {
        label: "初期読み込み時間",
        value: "21.090",
        unit: "ms",
        updatedAt: "2026-04-19 23:44",
        previousValue: "21.090",
        delta: "+0.000",
      },
    ]);
  });
});
