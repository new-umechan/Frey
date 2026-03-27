import { DELTA_FIELD_KIND_BY_VIEW, type FieldKind } from "./constants";

export function getDeltaFieldKindsForView(options: {
    viewMode?: string;
    cellMetric?: string;
}): FieldKind[] {
    const { viewMode = "normal", cellMetric = "height" } = options;
    if (viewMode === "metric") {
        return ["height", "river_flux", "river_next", cellMetric as FieldKind];
    }
    return (DELTA_FIELD_KIND_BY_VIEW[viewMode] ?? DELTA_FIELD_KIND_BY_VIEW.normal) as FieldKind[];
}
