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

export async function persistTheme(theme: UiTheme): Promise<void> {
  applyTheme(theme);
  await invoke("set_setting", { key: "ui_theme", value: theme });
}
