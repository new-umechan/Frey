import { DELTA_FIELD_KIND_BY_VIEW, type FieldKind } from "./constants";
import { getOverlayFieldKindForMetric } from "../../visualizers/cell-metric";

export function getDeltaFieldKindsForView(options: {
    viewMode?: string;
    cellMetric?: string;
}): FieldKind[] {
    const { viewMode = "normal", cellMetric = "height" } = options;
    if (viewMode === "metric") {
        const fields: FieldKind[] = ["height", "lake_depth", "river_flux", "river_next", cellMetric as FieldKind];
        const overlayField = getOverlayFieldKindForMetric(cellMetric);
        if (overlayField && !fields.includes(overlayField as FieldKind)) {
            fields.push(overlayField as FieldKind);
        }
        return fields;
    }
    return (DELTA_FIELD_KIND_BY_VIEW[viewMode] ?? DELTA_FIELD_KIND_BY_VIEW.normal) as FieldKind[];
}
