import { getCellMetricMeta } from "../cell-metric.js";
import { DELTA_FIELD_KIND_BY_VIEW } from "./constants.js";

export function getDeltaFieldKindsForView({ viewMode, cellMetric }) {
    if (viewMode === "metric") {
        const meta = getCellMetricMeta(cellMetric);
        return ["height", "river_flux", "river_next", meta.fieldKind];
    }
    return DELTA_FIELD_KIND_BY_VIEW[viewMode] ?? DELTA_FIELD_KIND_BY_VIEW.normal;
}
