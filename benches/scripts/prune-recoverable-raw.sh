#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage:
    benches/scripts/prune-recoverable-raw.sh [--apply]

Options:
    --apply   Actually delete targets. Without this option, runs in dry-run mode.
EOF
}

ensure_repo_root() {
    if [ ! -f "package.json" ] || [ ! -d "bench" ]; then
        echo "error: run this script from repository root." >&2
        exit 1
    fi
}

size_of_path() {
    local path="$1"
    du -sh "${path}" 2>/dev/null | awk '{print $1}'
}

main() {
    ensure_repo_root

    local apply="false"
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --apply)
                apply="true"
                shift
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                echo "error: unknown option: $1" >&2
                usage
                exit 1
                ;;
        esac
    done

    local targets=(
        "benches/raw/archive"
        "benches/raw/climate/era5_land_monthly_1970_2000.zip"
        "benches/raw/climate/era5_land_annual_1970_2000.nc"
        "benches/raw/hydrology/glofas_raw"
        "benches/raw/hydrology/glofas_era5_annual_mean.nc"
        "benches/raw/ecology/soilgrids"
    )

    local found_any="false"
    echo "mode: $( [ "${apply}" = "true" ] && echo "apply" || echo "dry-run" )"
    echo "targets:"

    for path in "${targets[@]}"; do
        if [ -e "${path}" ]; then
            found_any="true"
            local sz
            sz="$(size_of_path "${path}")"
            echo "  - ${path} (${sz:-unknown})"
        fi
    done

    if [ "${found_any}" != "true" ]; then
        echo "no recoverable targets found."
        exit 0
    fi

    if [ "${apply}" != "true" ]; then
        echo "dry-run complete. use --apply to delete targets."
        exit 0
    fi

    for path in "${targets[@]}"; do
        if [ -e "${path}" ]; then
            rm -rf "${path}"
            echo "deleted: ${path}"
        fi
    done

    echo "prune complete."
}

main "$@"
