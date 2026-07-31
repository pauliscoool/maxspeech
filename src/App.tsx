import { useState, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import Shell from "./windows/Shell";
import Overlay from "./windows/Overlay";
import Onboarding from "./windows/Onboarding";
import AuthPanel from "./components/AuthPanel";
import { applyTheme, loadAndApplyTheme } from "./lib/theme";
import { getSessionUser, onAuthChange, type AuthUser } from "./lib/auth";
import { pullCloudSettings } from "./lib/cloudSync";

type View = "overlay" | "shell" | "onboarding";

function initialView(): View {
  try {
    const label = getCurrentWindow().label;
    if (label === "overlay") return "overlay";
    if (label === "onboarding") return "onboarding";
  } catch {
    // Browser/dev without Tauri
  }
  return "shell";
}

export default function App() {
  const [view, setView] = useState<View>(initialView);
  const [authUser, setAuthUser] = useState<AuthUser | null>(null);
  const [authReady, setAuthReady] = useState(false);

  useEffect(() => {
    const label = getCurrentWindow().label;
    if (label === "overlay") {
      document.documentElement.setAttribute("data-window", "overlay");
      applyTheme("dark");
      const clear = [0, 0, 0, 0] as [number, number, number, number];
      void getCurrentWindow()
        .setBackgroundColor(clear)
        .catch(() => {});
      void getCurrentWebview()
        .setBackgroundColor(clear)
        .catch(() => {});
      setView("overlay");
      setAuthReady(true);
      return;
    }
    if (label === "onboarding") {
      document.documentElement.setAttribute("data-window", "onboarding");
      setView("onboarding");
    } else {
      document.documentElement.setAttribute("data-window", "shell");
      setView("shell");
    }
    void loadAndApplyTheme();

    let cancelled = false;
    void (async () => {
      try {
        const user = await getSessionUser();
        if (cancelled) return;
        setAuthUser(user);
        if (user) await pullCloudSettings();
      } catch {
        if (!cancelled) setAuthUser(null);
      } finally {
        if (!cancelled) setAuthReady(true);
      }
    })();

    const unsub = onAuthChange((u) => {
      setAuthUser(u);
      setAuthReady(true);
    });
    return () => {
      cancelled = true;
      unsub();
    };
  }, []);

  if (view === "overlay") return <Overlay />;

  if (!authReady) {
    return (
      <div className="flex h-screen items-center justify-center bg-[var(--ms-bg)] text-[var(--ms-text-dim)] text-sm">
        Loading…
      </div>
    );
  }

  // Shell requires a signed-in account (onboarding handles its own account step)
  if (view === "shell" && !authUser) {
    return (
      <div className="flex items-center justify-center h-screen bg-[var(--ms-bg)] text-[var(--ms-text)] relative overflow-hidden">
        <div
          className="pointer-events-none absolute -left-24 bottom-0 w-72 h-72 rounded-full blur-3xl opacity-30"
          style={{ background: "radial-gradient(circle, var(--ms-orange), transparent 70%)" }}
        />
        <div
          className="pointer-events-none absolute -right-20 top-0 w-80 h-80 rounded-full blur-3xl opacity-25"
          style={{ background: "radial-gradient(circle, var(--ms-turquoise), transparent 70%)" }}
        />
        <div className="relative z-10 p-8 w-full flex justify-center">
          <AuthPanel
            onAuthed={(u) => {
              setAuthUser(u);
              void pullCloudSettings();
            }}
          />
        </div>
      </div>
    );
  }

  if (view === "onboarding") return <Onboarding />;
  return <Shell authUser={authUser} />;
}
