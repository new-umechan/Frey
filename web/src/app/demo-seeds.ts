export interface DemoSeed {
    seed: string;
    label: string;
    mesh_level?: number;
    description?: string;
}

interface DemoSeedManifest {
    seeds?: unknown;
}

function readViteEnv(key: string): string {
    const env = (import.meta as unknown as { env?: Record<string, unknown> }).env;
    const value = env?.[key];
    return typeof value === "string" ? value.trim() : "";
}

function normalizeDemoSeed(value: unknown): DemoSeed | null {
    if (!value || typeof value !== "object") {
        return null;
    }
    const record = value as Record<string, unknown>;
    const seed = typeof record.seed === "string" ? record.seed.trim() : "";
    if (!seed) {
        return null;
    }
    const label = typeof record.label === "string" && record.label.trim()
        ? record.label.trim()
        : seed;
    const description = typeof record.description === "string" && record.description.trim()
        ? record.description.trim()
        : undefined;
    const meshLevel = typeof record.mesh_level === "number" && Number.isFinite(record.mesh_level)
        ? record.mesh_level
        : undefined;
    return {
        seed,
        label,
        mesh_level: meshLevel,
        description,
    };
}

function normalizeDemoSeeds(manifest: DemoSeedManifest): DemoSeed[] {
    const rawSeeds = Array.isArray(manifest.seeds) ? manifest.seeds : [];
    const seeds = rawSeeds
        .map(normalizeDemoSeed)
        .filter((seed): seed is DemoSeed => seed !== null);
    const seen = new Set<string>();
    return seeds.filter((seed) => {
        if (seen.has(seed.seed)) {
            return false;
        }
        seen.add(seed.seed);
        return true;
    });
}

export function getDemoSeedsUrl(): string {
    return readViteEnv("VITE_FREY_DEMO_SEEDS_URL");
}

export async function loadDemoSeeds(): Promise<DemoSeed[]> {
    const url = getDemoSeedsUrl();
    if (!url) {
        return [];
    }
    const response = await fetch(url, { cache: "no-store" });
    if (!response.ok) {
        throw new Error(`demo seed manifest failed: HTTP ${response.status}`);
    }
    const manifest = await response.json() as DemoSeedManifest;
    return normalizeDemoSeeds(manifest);
}

export function renderDemoSeedSelector(options: {
    form: HTMLFormElement;
    input: HTMLInputElement;
    seeds: DemoSeed[];
    onSelect?: (seed: string) => void;
}) {
    const { form, input, seeds, onSelect } = options;
    if (seeds.length === 0) {
        form.classList.remove("is-demo-seed-mode");
        input.removeAttribute("readonly");
        return;
    }

    const selectedSeed = seeds.some((seed) => seed.seed === input.value)
        ? input.value
        : seeds[0].seed;
    input.value = selectedSeed;
    input.setAttribute("readonly", "readonly");
    form.classList.add("is-demo-seed-mode");
    form.querySelector(".demo-seed-list")?.remove();

    const list = document.createElement("div");
    list.className = "demo-seed-list";
    list.setAttribute("role", "radiogroup");
    list.setAttribute("aria-label", "公開デモ seed");

    for (const seed of seeds) {
        const button = document.createElement("button");
        button.className = "demo-seed-option";
        button.type = "button";
        button.dataset.seed = seed.seed;
        button.setAttribute("role", "radio");
        button.setAttribute("aria-checked", String(seed.seed === selectedSeed));

        const label = document.createElement("span");
        label.className = "demo-seed-label";
        label.textContent = seed.label;
        button.append(label);

        if (seed.description) {
            const description = document.createElement("span");
            description.className = "demo-seed-description";
            description.textContent = seed.description;
            button.append(description);
        }

        button.addEventListener("click", () => {
            if (input.value === seed.seed) {
                return;
            }
            input.value = seed.seed;
            for (const item of list.querySelectorAll(".demo-seed-option")) {
                const isSelected = item instanceof HTMLButtonElement && item.dataset.seed === seed.seed;
                item.setAttribute("aria-checked", String(isSelected));
            }
            onSelect?.(seed.seed);
            form.requestSubmit();
        });
        list.append(button);
    }

    form.append(list);
}
