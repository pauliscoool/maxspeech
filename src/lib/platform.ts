export type OsKind = "windows" | "macos" | "linux" | "unknown";

/** Best-effort OS detect inside the Tauri WebView. */
export function detectOs(): OsKind {
  const ua = navigator.userAgent.toLowerCase();
  const plat = (navigator.platform || "").toLowerCase();
  if (plat.includes("win") || ua.includes("windows")) return "windows";
  if (plat.includes("mac") || ua.includes("mac")) return "macos";
  if (plat.includes("linux") || ua.includes("linux")) return "linux";
  return "unknown";
}

export function defaultHotkey(): string {
  return detectOs() === "windows" ? "ctrl+super" : "ctrl+shift+space";
}

/** Label for the Super/Meta/Cmd key on this OS. */
export function superKeyLabel(os: OsKind = detectOs()): string {
  if (os === "macos") return "Cmd";
  if (os === "linux") return "Super";
  return "Win";
}

export function formatHotkey(raw: string, os: OsKind = detectOs()): string {
  const superLabel = superKeyLabel(os);
  return raw
    .split("+")
    .map((p) => {
      const t = p.trim().toLowerCase();
      if (t === "ctrl" || t === "control") return "Ctrl";
      if (t === "alt" || t === "option") return os === "macos" ? "Option" : "Alt";
      if (t === "shift") return "Shift";
      if (t === "super" || t === "meta" || t === "cmd" || t === "command" || t === "win") {
        return superLabel;
      }
      if (t === "space") return "Space";
      return t.length === 1 ? t.toUpperCase() : t.charAt(0).toUpperCase() + t.slice(1);
    })
    .join(" + ");
}
