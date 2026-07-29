import { useState, useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Settings from "./windows/Settings";
import Overlay from "./windows/Overlay";
import Onboarding from "./windows/Onboarding";
import History from "./windows/History";
import Transcriber from "./windows/Transcriber";

type View = "overlay" | "settings" | "onboarding" | "history" | "transcriber";

export default function App() {
  const [view, setView] = useState<View>("overlay");

  useEffect(() => {
    const label = getCurrentWindow().label;
    if (label === "settings") setView("settings");
    else if (label === "onboarding") setView("onboarding");
    else if (label === "history") setView("history");
    else if (label === "transcriber") setView("transcriber");
    else setView("overlay");
  }, []);

  switch (view) {
    case "settings":
      return <Settings />;
    case "onboarding":
      return <Onboarding />;
    case "history":
      return <History />;
    case "transcriber":
      return <Transcriber />;
    default:
      return <Overlay />;
  }
}
