import { describe, expect, it } from "vitest";
import { biomeLegendItems, formatBiomeLabel, getCellMetricMeta } from "./cell-metric";

describe("気候種の凡例", () => {
    it("各気候種に一意な色と表示名を持つ", () => {
        const items = biomeLegendItems();

        expect(items).toHaveLength(9);
        expect(new Set(items.map((item) => item.color)).size).toBe(items.length);
        expect(items.map((item) => item.label)).toContain(formatBiomeLabel(0));
        expect(items.map((item) => item.label)).toContain(formatBiomeLabel(8));
    });
});

describe("作物・家畜レイヤー名", () => {
    it("接頭辞なしの日本語名称を表示する", () => {
        expect(getCellMetricMeta("crop_adoption_wheat").label).toBe("小麦");
        expect(getCellMetricMeta("crop_adoption_maize").label).toBe("トウモロコシ");
        expect(getCellMetricMeta("livestock_adoption_cattle").label).toBe("牛");
        expect(getCellMetricMeta("livestock_adoption_camel").label).toBe("ラクダ");
    });
});
