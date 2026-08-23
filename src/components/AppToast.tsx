import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import {
  TOAST_EVENT,
  type AppToastDetail,
  type AppToastKind,
} from "../lib/toast";

const SHOW_MS = 3200;
const OUT_MS = 240;

type ToastState = {
  id: number;
  message: string;
  kind: AppToastKind;
};

export default function AppToast() {
  const [toast, setToast] = useState<ToastState | null>(null);
  const [leaving, setLeaving] = useState(false);

  useEffect(() => {
    const onToast = (ev: Event) => {
      const detail = (ev as CustomEvent<AppToastDetail>).detail;
      const message = detail?.message?.trim();
      if (!message) return;
      setLeaving(false);
      setToast({
        id: Date.now(),
        message,
        kind: detail.kind ?? "success",
      });
    };
    window.addEventListener(TOAST_EVENT, onToast);
    return () => window.removeEventListener(TOAST_EVENT, onToast);
  }, []);

  useEffect(() => {
    if (!toast) return;
    setLeaving(false);
    const hide = window.setTimeout(() => setLeaving(true), SHOW_MS);
    const clear = window.setTimeout(() => setToast(null), SHOW_MS + OUT_MS);
    return () => {
      window.clearTimeout(hide);
      window.clearTimeout(clear);
    };
  }, [toast?.id]);

  if (!toast || typeof document === "undefined" || !document.body) return null;

  return createPortal(
    <div
      className={`app-toast ${leaving ? "app-toast--out" : "app-toast--in"}`}
      role="status"
      aria-live="polite"
    >
      {toast.kind === "success" ? <GreenCheck /> : null}
      <p className="app-toast-copy">{toast.message}</p>
    </div>,
    document.body,
  );
}

function GreenCheck() {
  return (
    <span className="app-toast-check" aria-hidden>
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none">
        <circle cx="12" cy="12" r="11" fill="currentColor" />
        <path
          d="M7.2 12.4l3.1 3.2 6.5-7.1"
          stroke="#fff"
          strokeWidth="2.15"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    </span>
  );
}
