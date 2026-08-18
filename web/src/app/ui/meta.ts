/**
 * 右上メタ(Figma 357-94)の seed inline 編集。
 * 通常は "seed = <値>" を表示し、クリックで既存の #seed-form の入力に切り替える。
 * seed の反映(world 再生成)は既存の submit ハンドラが行う。ここは表示と開閉のみ。
 */
export function setupMeta(): void {
    const display = document.getElementById("seed-display");
    const form = document.getElementById("seed-form");
    const input = document.getElementById("seed-input");
    if (
        !(display instanceof HTMLElement) ||
        !(form instanceof HTMLFormElement) ||
        !(input instanceof HTMLInputElement)
    ) {
        return;
    }

    // 表示に出す確定済みの seed。blur で未確定の入力を巻き戻すために保持する。
    let committed = input.value;

    const renderDisplay = () => {
        display.textContent = `seed = ${committed}`;
    };

    const openEditor = () => {
        input.value = committed;
        form.hidden = false;
        display.hidden = true;
        input.focus();
        input.select();
    };

    const closeEditor = () => {
        form.hidden = true;
        display.hidden = false;
        renderDisplay();
    };

    display.addEventListener("click", openEditor);

    // 確定は既存の submit ハンドラに任せ、ここでは表示値を更新して閉じる。
    form.addEventListener("submit", () => {
        committed = input.value;
        // 既存ハンドラの処理後に表示へ戻す。
        window.setTimeout(closeEditor, 0);
    });

    // フォーカスを外したら未確定の入力は捨てて表示へ戻す。
    input.addEventListener("blur", () => {
        input.value = committed;
        closeEditor();
    });

    renderDisplay();
}
