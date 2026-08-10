import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import {
  getCurrentWindow,
  currentMonitor,
  LogicalSize,
  LogicalPosition,
} from "@tauri-apps/api/window";

type DictationState = "idle" | "listening" | "processing" | "done" | "error" | "limit";

interface TranscriptEvent {
  text: string;
  is_final: boolean;
}

interface EnhanceEvent {
  original: string;
  enhanced: string;
}

/** Sized to actually fit inside the narrower pill without clipping/overflow
 *  against the status label — that mismatch was causing the "weird" layout. */
const BAR_COUNT = 20;
const BAR_MAX_PX = 17;
const BAR_WIDTH_PX = 2;
const BAR_GAP_PX = 1.8;
/** Old pill was 218×38. ~20% narrower sides, ~5% less height — not taller. */
const OVERLAY_W = 174;
const OVERLAY_H = 36;
const PILL_OUT_MS = 100;
const CLEAR_BG = [18, 18, 18, 0] as [number, number, number, number];
const TOAST_W = 220;
const TOAST_H = 90;
const TOAST_MS = 1800;
const TOAST_OUT_MS = 160;
const LIMIT_W = 292;
const LIMIT_H = 128;
const LIMIT_MS = 10000;
const ERROR_MS = 5000;

export default function Overlay() {
  const [state, setState] = useState<DictationState>("idle");
  const [transcript, setTranscript] = useState("");
  const [error, setError] = useState("");
  const [levels, setLevels] = useState<number[]>(() => Array(BAR_COUNT).fill(0.14));
  const [showLive, setShowLive] = useState(true);
  const [toast, setToast] = useState<EnhanceEvent | null>(null);
  const [toastLeaving, setToastLeaving] = useState(false);
  const [pillLeaving, setPillLeaving] = useState(false);
  const smoothed = useRef<number[]>(Array(BAR_COUNT).fill(0.14));
  const raf = useRef<number | null>(null);
  const listening = useRef(false);
  const toastTimer = useRef<number | null>(null);
  const toastActive = useRef(false);
  const pillOutTimer = useRef<number | null>(null);
  const limitTimer = useRef<number | null>(null);
  const errorTimer = useRef<number | null>(null);

  function hidePillSoon() {
    if (pillOutTimer.current) window.clearTimeout(pillOutTimer.current);
    // Park off-screen first so any leave fade never reveals WebView2 chrome.
    void resizeForState("idle");
    setPillLeaving(true);
    pillOutTimer.current = window.setTimeout(() => {
      setPillLeaving(false);
    }, PILL_OUT_MS);
  }

  useEffect(() => {
    // Warm path: transparent click-through shell stays shown at bottom-center.
    void ensureScreen();
    void clearOverlayChrome();
    void resizeForState("idle");
    invoke<string>("get_setting", { key: "show_live_transcript" })
      .then((v) => setShowLive(v !== "false"))
      .catch(() => setShowLive(true));
  }, []);

  useEffect(() => {
    if (state !== "listening") {
      if (raf.current) cancelAnimationFrame(raf.current);
      raf.current = null;
      listening.current = false;
      return;
    }
    listening.current = true;
    let t0 = performance.now();
    let lastTick = 0;
    const FRAME_MS = 1000 / 30;
    const tick = (now: number) => {
      if (!listening.current) return;
      if (now - lastTick < FRAME_MS) {
        raf.current = requestAnimationFrame(tick);
        return;
      }
      lastTick = now;
      const t = (now - t0) / 1000;
      const avg =
        smoothed.current.reduce((a, b) => a + b, 0) / smoothed.current.length;
      if (avg < 0.28) {
        const next = smoothed.current.map((v, i) => {
          // Slow, gentle idle wave — not twitchy.
          const wave =
            0.14 +
            0.22 * (0.5 + 0.5 * Math.sin(t * 3.6 + i * 0.48)) *
              (0.55 + 0.45 * Math.sin(t * 1.7 + i * 0.72));
          // Ease toward the wave instead of snapping up.
          return v + (Math.max(v, wave) - v) * 0.22;
        });
        smoothed.current = next;
        setLevels([...next]);
      }
      raf.current = requestAnimationFrame(tick);
    };
    raf.current = requestAnimationFrame(tick);
    return () => {
      listening.current = false;
      if (raf.current) cancelAnimationFrame(raf.current);
    };
  }, [state]);

  function clearToastSoon() {
    if (toastTimer.current) window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => {
      setToastLeaving(true);
      window.setTimeout(() => {
        setToast(null);
        setToastLeaving(false);
        toastActive.current = false;
        void resizeForState("idle");
      }, TOAST_OUT_MS);
    }, TOAST_MS);
  }

  function clearLimitSoon() {
    if (limitTimer.current) window.clearTimeout(limitTimer.current);
    limitTimer.current = window.setTimeout(() => {
      setState("idle");
      setError("");
      void resizeForState("idle");
    }, LIMIT_MS);
  }

  function clearErrorSoon() {
    if (errorTimer.current) window.clearTimeout(errorTimer.current);
    errorTimer.current = window.setTimeout(() => {
      setState("idle");
      setError("");
      void resizeForState("idle");
    }, ERROR_MS);
  }

  async function openUpgradeSettings() {
    if (limitTimer.current) window.clearTimeout(limitTimer.current);
    if (errorTimer.current) window.clearTimeout(errorTimer.current);
    setState("idle");
    setError("");
    void resizeForState("idle");
    try {
      await invoke("open_plans_modal");
    } catch (e) {
      console.error("Failed to open plans", e);
    }
  }

  useEffect(() => {
    const unsubs: (() => void)[] = [];

    listen<string>("dictation-state", (e) => {
      const next = e.payload as DictationState;
      setState(next);
      if (next === "idle") {
        setTranscript("");
        setError("");
        smoothed.current = Array(BAR_COUNT).fill(0.14);
        setLevels(Array(BAR_COUNT).fill(0.14));
        if (limitTimer.current) window.clearTimeout(limitTimer.current);
        if (errorTimer.current) window.clearTimeout(errorTimer.current);
        if (!toastActive.current) hidePillSoon();
      } else if (next === "limit") {
        setToast(null);
        setToastLeaving(false);
        setPillLeaving(false);
        toastActive.current = false;
        if (toastTimer.current) window.clearTimeout(toastTimer.current);
        if (pillOutTimer.current) window.clearTimeout(pillOutTimer.current);
        if (errorTimer.current) window.clearTimeout(errorTimer.current);
        void resizeForState("limit");
        clearLimitSoon();
      } else if (next === "error") {
        setToast(null);
        setToastLeaving(false);
        setPillLeaving(false);
        toastActive.current = false;
        if (toastTimer.current) window.clearTimeout(toastTimer.current);
        if (pillOutTimer.current) window.clearTimeout(pillOutTimer.current);
        if (limitTimer.current) window.clearTimeout(limitTimer.current);
        void resizeForState("error");
        clearErrorSoon();
      } else if (next === "listening" || next === "processing") {
        // Cancel any leave animation from a prior session so the pill snaps back.
        setToast(null);
        setToastLeaving(false);
        setPillLeaving(false);
        toastActive.current = false;
        if (toastTimer.current) window.clearTimeout(toastTimer.current);
        if (pillOutTimer.current) {
          window.clearTimeout(pillOutTimer.current);
          pillOutTimer.current = null;
        }
        if (limitTimer.current) window.clearTimeout(limitTimer.current);
        if (errorTimer.current) window.clearTimeout(errorTimer.current);
        setError("");
        void resizeForState(next);
      } else {
        setPillLeaving(false);
        void resizeForState(next, toastActive.current);
      }
      invoke<string>("get_setting", { key: "show_live_transcript" })
        .then((v) => setShowLive(v !== "false"))
        .catch(() => {});
    }).then((u) => unsubs.push(u));

    listen<TranscriptEvent>("transcript", (e) => {
      setTranscript(e.payload.text);
    }).then((u) => unsubs.push(u));

    listen<string>("dictation-error", (e) => {
      setError(e.payload);
      setState("error");
      void resizeForState("error");
      clearErrorSoon();
    }).then((u) => unsubs.push(u));

    listen("dictation-limit", () => {
      setError("");
      setState("limit");
      void resizeForState("limit");
      clearLimitSoon();
    }).then((u) => unsubs.push(u));

    listen<EnhanceEvent>("dictation-enhanced", (e) => {
      const payload = e.payload;
      if (!payload?.enhanced || payload.original === payload.enhanced) return;
      toastActive.current = true;
      setToast(payload);
      setToastLeaving(false);
      void resizeForState("done", true);
      clearToastSoon();
    }).then((u) => unsubs.push(u));

    listen<number[]>("audio-level", (e) => {
      const incoming = Array.isArray(e.payload) ? e.payload : [];
      const next = Array.from({ length: BAR_COUNT }, (_, i) => {
        const src =
          incoming.length === BAR_COUNT
            ? Number(incoming[i] ?? 0.08)
            : Number(
                incoming[
                  Math.round((i / Math.max(1, BAR_COUNT - 1)) * Math.max(0, incoming.length - 1))
                ] ?? 0.08,
              );
        const target = Math.max(0.06, Math.min(1, src));
        const prev = smoothed.current[i] ?? 0.14;
        // Responsive enough to show speech, still smoother than raw peaks.
        const alpha = target > prev ? 0.48 : 0.26;
        const v = prev + (target - prev) * alpha;
        smoothed.current[i] = v;
        return v;
      });
      setLevels(next);
    }).then((u) => unsubs.push(u));

    return () => {
      unsubs.forEach((u) => u());
      if (toastTimer.current) window.clearTimeout(toastTimer.current);
      if (pillOutTimer.current) window.clearTimeout(pillOutTimer.current);
      if (limitTimer.current) window.clearTimeout(limitTimer.current);
      if (errorTimer.current) window.clearTimeout(errorTimer.current);
    };
  }, []);

  const showToast = !!toast;
  const showLimit = state === "limit";
  // Keep the pill mounted even while idle (parked off-screen). Unmounting left
  // an empty transparent root; Rust show_overlay_fast could reveal WebView2
  // white for a frame before React remounted the charcoal pill.
  const showPill =
    !showToast &&
    (showLimit ||
      state === "listening" ||
      state === "processing" ||
      state === "done" ||
      state === "error" ||
      state === "idle" ||
      pillLeaving);

  const statusLabel =
    state === "listening"
      ? "Listening…"
      : state === "processing"
        ? "Enhancing…"
        : state === "done"
          ? "Done"
          : state === "limit"
            ? "Weekly limit reached"
            : state === "error"
              ? "Error"
              : state === "idle"
                ? "Listening…"
                : "";

  const label =
    state === "limit"
      ? "Weekly limit reached"
      : error || (showLive && transcript ? transcript : statusLabel);

  const snippet = toast
    ? truncate(toast.original, 36) + " → " + truncate(toast.enhanced, 36)
    : "";

  const pillActive =
    state === "listening" ||
    state === "processing" ||
    state === "done" ||
    state === "error" ||
    showLimit ||
    pillLeaving;

  return (
    <div
      className={`overlay-root ${
        showToast ? "overlay-root--toast" : showLimit ? "overlay-root--limit" : ""
      }`}
    >
      {showLimit && (
        <button
          type="button"
          className="limit-upgrade liquid-glass-toast"
          onClick={() => void openUpgradeSettings()}
        >
          <div className="ms-logo ms-logo--toast shrink-0" aria-hidden>
            <img
              src="/logo.png"
              srcSet="/logo.png 1x, /logo@2x.png 2x"
              alt=""
              width={29}
              height={29}
              draggable={false}
            />
          </div>
          <div className="min-w-0 flex-1 text-left">
            <div className="enhance-toast-title">Subscribe to a higher tier</div>
            <div className="enhance-toast-sub">Tap to choose a plan</div>
          </div>
        </button>
      )}

      {showToast && (
        <div
          className={`enhance-toast liquid-glass-toast ${
            toastLeaving ? "enhance-toast--out" : "enhance-toast--in"
          }`}
        >
          <div className="ms-logo ms-logo--toast shrink-0" aria-hidden>
            <img
              src="/logo.png"
              srcSet="/logo.png 1x, /logo@2x.png 2x"
              alt=""
              width={29}
              height={29}
              draggable={false}
            />
          </div>
          <div className="min-w-0 flex-1">
            <div className="enhance-toast-title">Fixed grammar</div>
            <div className="enhance-toast-sub">Enhanced by MaxSpeech</div>
            {snippet && <div className="enhance-toast-snip">{snippet}</div>}
          </div>
        </div>
      )}

      {showPill && (
        <div
          data-tauri-drag-region
          className={`liquid-glass-pill flex items-center gap-1.5 px-2.5 py-1 rounded-full select-none ${
            showLimit ? "liquid-glass-pill--limit" : "w-full h-full"
          } ${
            pillLeaving
              ? "liquid-glass-pill--out"
              : pillActive && state !== "idle"
                ? "liquid-glass-pill--in"
                : ""
          }`}
        >
          {!showLimit && (
            <div
              className="flex items-end justify-center flex-1 min-w-0"
              style={{ gap: `${BAR_GAP_PX}px`, height: `${BAR_MAX_PX}px` }}
            >
              {levels.map((level, i) => {
                const mid =
                  1 -
                  (Math.abs(i - (BAR_COUNT - 1) / 2) / ((BAR_COUNT - 1) / 2)) * 0.18;
                const px = Math.max(2.5, level * mid * BAR_MAX_PX);
                const isOrange = i % 6 === 3;
                return (
                  <div
                    key={i}
                    className="liquid-glass-bar shrink-0 origin-bottom"
                    style={{
                      height: `${px.toFixed(2)}px`,
                      width: `${BAR_WIDTH_PX}px`,
                      ["--bar-color" as string]: isOrange
                        ? "var(--ms-orange)"
                        : "var(--ms-turquoise)",
                      ["--bar-hi" as string]: isOrange ? "#fdba74" : "#99f6e4",
                    }}
                  />
                );
              })}
            </div>
          )}

          <span
            className={`font-medium truncate ${
              showLimit
                ? "text-[10px] text-white/90 w-full text-center"
                : "text-[8px] text-white/85 max-w-[64px]"
            }`}
          >
            {label}
          </span>
        </div>
      )}
    </div>
  );
}

function truncate(s: string, n: number) {
  const t = s.trim().replace(/\s+/g, " ");
  return t.length <= n ? t : t.slice(0, n - 1) + "…";
}

/** Cached monitor size so hotkey resize doesn't wait on currentMonitor every time. */
let cachedScreen: { w: number; h: number } | null = null;

async function clearOverlayChrome() {
  const win = getCurrentWindow();
  try {
    await win.setBackgroundColor(CLEAR_BG);
  } catch {
    /* ignore */
  }
  try {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    await getCurrentWebview().setBackgroundColor(CLEAR_BG);
  } catch {
    /* ignore */
  }
}

async function ensureScreen(): Promise<{ w: number; h: number } | null> {
  if (cachedScreen) return cachedScreen;
  const monitor = await currentMonitor();
  if (!monitor) return null;
  const scale = monitor.scaleFactor;
  cachedScreen = {
    w: monitor.size.width / scale,
    h: monitor.size.height / scale,
  };
  return cachedScreen;
}

async function positionBottomCenter(w: number, h: number) {
  try {
    const win = getCurrentWindow();
    const screen = await ensureScreen();
    if (!screen) return;
    // Await chrome clear before paint — fire-and-forget left WebView2 white up.
    await clearOverlayChrome();
    await Promise.all([
      win.setSize(new LogicalSize(w, h)),
      win.setPosition(
        new LogicalPosition((screen.w - w) / 2, screen.h - h - 48),
      ),
      win.setAlwaysOnTop(true),
    ]);
    // Clear any leftover GDI region; CSS border-radius draws the smooth pill.
    void invoke("set_overlay_pill_clip", { apply: false }).catch(() => {});
  } catch (e) {
    console.error("Failed to position overlay", e);
  }
}

/** Keep the overlay WebView shown (never hide) so Windows doesn't cold-wake it on hotkey.
 *  Idle parks off-screen — avoids a visible strip if DWM fails to composite alpha. */
async function resizeForState(state: DictationState, withToast = false) {
  try {
    const win = getCurrentWindow();
    await clearOverlayChrome();
    if (state === "idle" && !withToast) {
      void invoke("set_overlay_pill_clip", { apply: false }).catch(() => {});
      await Promise.all([
        win.setIgnoreCursorEvents(true).catch(() => {}),
        win.setSize(new LogicalSize(1, 1)),
        win.setPosition(new LogicalPosition(-40_000, -40_000)),
        win.show(),
      ]);
      await clearOverlayChrome();
      return;
    }
    const needsClicks = state === "limit" || withToast;
    // Geometry first while chrome is cleared; show only after size is ready.
    if (state === "limit") {
      await positionBottomCenter(LIMIT_W, LIMIT_H);
    } else if (withToast) {
      await positionBottomCenter(TOAST_W, TOAST_H);
    } else {
      await positionBottomCenter(OVERLAY_W, OVERLAY_H);
    }
    await Promise.all([
      win.setIgnoreCursorEvents(!needsClicks).catch(() => {}),
      win.show(),
    ]);
    await clearOverlayChrome();
  } catch {
    // ignore
  }
}
