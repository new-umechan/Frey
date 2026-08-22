import { describe, expect, it } from "vitest";
import { biomeLegendItems, formatBiomeLabel } from "./cell-metric";

describe("気候種の凡例", () => {
    it("各気候種に一意な色と表示名を持つ", () => {
        const items = biomeLegendItems();

        expect(items).toHaveLength(9);
        expect(new Set(items.map((item) => item.color)).size).toBe(items.length);
        expect(items.map((item) => item.label)).toContain(formatBiomeLabel(0));
        expect(items.map((item) => item.label)).toContain(formatBiomeLabel(8));
    });
});
