import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import "./index.css";

// Mark window role before first paint so CSS can keep overlay clear
// and paint solid chrome only for shell/onboarding.
try {
  const label = getCurrentWindow().label;
  if (label === "overlay") {
    document.documentElement.setAttribute("data-window", "overlay");
  } else if (label === "onboarding") {
    document.documentElement.setAttribute("data-window", "onboarding");
  } else {
    document.documentElement.setAttribute("data-window", "shell");
  }
} catch {
  document.documentElement.setAttribute("data-window", "shell");
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
