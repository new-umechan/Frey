/**
 * v2 フローティングパネル(レイヤー / 因果関係)の開閉・移動・リサイズ。
 * - サイドバーのアイコンボタンでトグルし、パネルの × で閉じる。
 * - タイトルバーをドラッグして移動、端/角をドラッグしてリサイズ。
 * 参照: docs/decisions/260724-causal-story-cross-module-trace.md, Figma 357-95 / 357-96
 */

interface PanelBinding {
    buttonId: string;
    panelId: string;
}

const PANEL_BINDINGS: PanelBinding[] = [
    { buttonId: "sidebar-layers-button", panelId: "layer-panel" },
    { buttonId: "sidebar-causal-button", panelId: "causal-panel" },
];

const RESIZE_DIRECTIONS = ["n", "s", "e", "w", "ne", "nw", "se", "sw"] as const;
const MIN_WIDTH = 220;
const MIN_HEIGHT = 120;

function setButtonActive(buttonId: string, active: boolean): void {
    document.getElementById(buttonId)?.classList.toggle("is-active", active);
}

/** CSS 由来の配置(top/left/right/bottom)を、px の left/top/width/height に固定する。 */
function ensureFreeform(panel: HTMLElement): void {
    if (panel.classList.contains("is-freeform")) {
        return;
    }
    const parent = panel.offsetParent as HTMLElement | null;
    const panelRect = panel.getBoundingClientRect();
    const parentRect = parent?.getBoundingClientRect();
    const offsetLeft = panelRect.left - (parentRect?.left ?? 0);
    const offsetTop = panelRect.top - (parentRect?.top ?? 0);
    panel.style.left = `${offsetLeft}px`;
    panel.style.top = `${offsetTop}px`;
    panel.style.width = `${panelRect.width}px`;
    panel.style.height = `${panelRect.height}px`;
    panel.classList.add("is-freeform");
}

/** ドラッグ中に move/up を捕捉する共通ループ。 */
function trackDrag(
    origin: HTMLElement,
    pointerId: number,
    onMove: (dx: number, dy: number) => void,
): void {
    origin.setPointerCapture(pointerId);
    const move = (event: PointerEvent) => {
        onMove(event.clientX, event.clientY);
    };
    const up = (event: PointerEvent) => {
        origin.releasePointerCapture(event.pointerId);
        origin.removeEventListener("pointermove", move);
        origin.removeEventListener("pointerup", up);
        origin.removeEventListener("pointercancel", up);
    };
    origin.addEventListener("pointermove", move);
    origin.addEventListener("pointerup", up);
    origin.addEventListener("pointercancel", up);
}

function makeDraggable(panel: HTMLElement, handle: HTMLElement): void {
    handle.addEventListener("pointerdown", (event) => {
        // タイトル内のボタン(× / +)はドラッグ開始しない。
        if ((event.target as HTMLElement).closest("button")) {
            return;
        }
        event.preventDefault();
        ensureFreeform(panel);
        const startX = event.clientX;
        const startY = event.clientY;
        const startLeft = panel.offsetLeft;
        const startTop = panel.offsetTop;
        trackDrag(handle, event.pointerId, (x, y) => {
            panel.style.left = `${startLeft + (x - startX)}px`;
            panel.style.top = `${startTop + (y - startY)}px`;
        });
    });
}

function makeResizable(panel: HTMLElement): void {
    for (const dir of RESIZE_DIRECTIONS) {
        const handle = document.createElement("div");
        handle.className = `v2-resize-handle v2-resize-handle--${dir}`;
        panel.appendChild(handle);

        handle.addEventListener("pointerdown", (event) => {
            event.preventDefault();
            event.stopPropagation();
            ensureFreeform(panel);
            const startX = event.clientX;
            const startY = event.clientY;
            const startLeft = panel.offsetLeft;
            const startTop = panel.offsetTop;
            const startWidth = panel.offsetWidth;
            const startHeight = panel.offsetHeight;

            trackDrag(handle, event.pointerId, (x, y) => {
                const dx = x - startX;
                const dy = y - startY;
                let left = startLeft;
                let top = startTop;
                let width = startWidth;
                let height = startHeight;

                if (dir.includes("e")) {
                    width = Math.max(MIN_WIDTH, startWidth + dx);
                }
                if (dir.includes("s")) {
                    height = Math.max(MIN_HEIGHT, startHeight + dy);
                }
                if (dir.includes("w")) {
                    width = Math.max(MIN_WIDTH, startWidth - dx);
                    left = startLeft + (startWidth - width);
                }
                if (dir.includes("n")) {
                    height = Math.max(MIN_HEIGHT, startHeight - dy);
                    top = startTop + (startHeight - height);
                }

                panel.style.left = `${left}px`;
                panel.style.top = `${top}px`;
                panel.style.width = `${width}px`;
                panel.style.height = `${height}px`;
            });
        });
    }
}

export function setupV2Panels(): void {
    for (const { buttonId, panelId } of PANEL_BINDINGS) {
        const button = document.getElementById(buttonId);
        const panel = document.getElementById(panelId);
        if (!button || !panel) {
            continue;
        }
        button.addEventListener("click", () => {
            const nowHidden = !panel.hidden;
            panel.hidden = nowHidden;
            setButtonActive(buttonId, !nowHidden);
        });

        const title = panel.querySelector<HTMLElement>(".v2-panel__title");
        if (title) {
            makeDraggable(panel, title);
        }
        makeResizable(panel);
    }

    // パネル内の × ボタン。
    for (const closer of document.querySelectorAll<HTMLElement>("[data-close]")) {
        closer.addEventListener("click", () => {
            const panelId = closer.getAttribute("data-close");
            if (!panelId) {
                return;
            }
            const panel = document.getElementById(panelId);
            if (panel) {
                panel.hidden = true;
            }
            const binding = PANEL_BINDINGS.find((b) => b.panelId === panelId);
            if (binding) {
                setButtonActive(binding.buttonId, false);
            }
        });
    }
}
