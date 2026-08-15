import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./index.css";

// Mark window role before first paint so CSS can keep overlay clear
// and paint solid chrome only for shell/onboarding.
let label = "shell";
try {
  const fromQuery = new URLSearchParams(window.location.search).get("window");
  label = fromQuery || getCurrentWindow().label;
} catch {
  label = "shell";
}
document.documentElement.setAttribute("data-window", label);

const root = document.getElementById("root")!;

// Overlay must stay a tiny bundle — importing App/Shell here would boot
// every settings page inside the hidden listening WebView.
if (label === "overlay") {
  void import("./windows/Overlay").then(({ default: Overlay }) => {
    ReactDOM.createRoot(root).render(<Overlay />);
  });
} else {
  void import("./App").then(({ default: App }) => {
    ReactDOM.createRoot(root).render(
      <React.StrictMode>
        <App />
      </React.StrictMode>,
    );
  });
}
