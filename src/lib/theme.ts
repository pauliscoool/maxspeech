import { invoke } from "@tauri-apps/api/core";

export type UiTheme = "dark" | "gray" | "light";

export const THEME_OPTIONS: {
  id: UiTheme;
  label: string;
  desc: string;
}[] = [
  {
    id: "dark",
    label: "Dark",
    desc: "Black surfaces, turquoise & orange",
  },
  {
    id: "gray",
    label: "Gray",
    desc: "Softer charcoal panels",
  },
  {
    id: "light",
    label: "Light",
    desc: "Soft light background",
  },
];

export function normalizeTheme(value: string | null | undefined): UiTheme {
  if (value === "gray" || value === "light" || value === "dark") return value;
  return "dark";
}

export function applyTheme(theme: UiTheme) {
  document.documentElement.setAttribute("data-theme", theme);
}

/** Play radial wipe, then apply theme mid-animation so chrome settles into the new look. */
export function transitionTheme(theme: UiTheme) {
  const current = normalizeTheme(
    document.documentElement.getAttribute("data-theme"),
  );
  if (current === theme) {
    applyTheme(theme);
    return;
  }
  window.dispatchEvent(
    new CustomEvent("ms-theme-wipe", { detail: { theme } }),
  );
  // Apply theme mid-wipe while the soft background disk is expanding (UI stays readable).
  window.setTimeout(() => applyTheme(theme), 220);
}

export async function loadAndApplyTheme(): Promise<UiTheme> {
  try {
    const value = await invoke<string>("get_setting", { key: "ui_theme" });
    const theme = normalizeTheme(value);
    applyTheme(theme);
    return theme;
  } catch {
    applyTheme("dark");
    return "dark";
  }
}

export async function persistTheme(
  theme: UiTheme,
  opts?: { animate?: boolean },
): Promise<void> {
  if (opts?.animate !== false) {
    transitionTheme(theme);
  } else {
    applyTheme(theme);
  }
  await invoke("set_setting", { key: "ui_theme", value: theme });
}
