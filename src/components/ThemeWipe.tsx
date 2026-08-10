import { useEffect, useState, type CSSProperties } from "react";
import type { UiTheme } from "../lib/theme";

const THEME_FILL: Record<UiTheme, string> = {
  dark: "#0a0a0a",
  gray: "#1a1a1c",
  light: "#f3f4f6",
};

type WipeState = {
  theme: UiTheme;
  key: number;
};

/**
 * Soft radial expand behind the shell chrome when the theme changes.
 * Background-only — never covers buttons/text (z-index under .ms-shell-chrome).
 * Listens for `ms-theme-wipe` CustomEvent with `{ theme: UiTheme }`.
 */
export default function ThemeWipe() {
  const [wipe, setWipe] = useState<WipeState | null>(null);

  useEffect(() => {
    const onWipe = (ev: Event) => {
      const detail = (ev as CustomEvent<{ theme: UiTheme }>).detail;
      if (!detail?.theme) return;
      setWipe({ theme: detail.theme, key: Date.now() });
    };
    window.addEventListener("ms-theme-wipe", onWipe);
    return () => window.removeEventListener("ms-theme-wipe", onWipe);
  }, []);

  useEffect(() => {
    if (!wipe) return;
    const done = window.setTimeout(() => setWipe(null), 720);
    return () => window.clearTimeout(done);
  }, [wipe]);

  if (!wipe) return null;

  return (
    <div
      key={wipe.key}
      className="ms-theme-wipe"
      aria-hidden
      style={
        {
          "--ms-wipe-fill": THEME_FILL[wipe.theme],
        } as CSSProperties
      }
    >
      <div className="ms-theme-wipe-dot" />
    </div>
  );
}
