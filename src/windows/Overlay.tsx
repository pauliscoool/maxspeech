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

interface EnhanceEvent {
  original: string;
  enhanced: string;
}

/** Calm teal pill — 20 bars, 3px wide, breathing room inside the capsule. */
const BAR_COUNT = 20;
const BAR_MAX_PX = 18;
const BAR_WIDTH_PX = 3;
const BAR_GAP_PX = 3;
/** After this long still processing, switch from ping-pong wave → digging sweep. */
const THINKING_DIG_AFTER_S = 3;
/** 20×3 + 19×3 = 117px bars + ~28px side padding. */
const OVERLAY_W = 148;
const OVERLAY_H = 36;
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
  const [levels, setLevels] = useState<number[]>(() => Array(BAR_COUNT).fill(0.14));
  const [toast, setToast] = useState<EnhanceEvent | null>(null);
  const [toastLeaving, setToastLeaving] = useState(false);
  const [pillLeaving, setPillLeaving] = useState(false);
  const [hearing, setHearing] = useState(false);
  const smoothed = useRef<number[]>(Array(BAR_COUNT).fill(0.14));
  const raf = useRef<number | null>(null);
  const listening = useRef(false);
  const hearingRef = useRef(false);
  const toastTimer = useRef<number | null>(null);
  const toastActive = useRef(false);
  const pillOutTimer = useRef<number | null>(null);
  const limitTimer = useRef<number | null>(null);
  const errorTimer = useRef<number | null>(null);

  function hidePillSoon() {
    if (pillOutTimer.current) window.clearTimeout(pillOutTimer.current);
    // Snap off-screen in Rust first — leave animations on-screen flash white.
    void invoke("park_overlay_idle").catch(() => resizeForState("idle"));
    setPillLeaving(false);
    pillOutTimer.current = null;
  }

  useEffect(() => {
    // Warm path: keep WebView shown, but park OFF-SCREEN while idle so a failed
    // alpha composite cannot leave a permanent white strip on the desktop.
    void ensureScreen();
    void clearOverlayChrome();
    void resizeForState("idle");
  }, []);

  useEffect(() => {
    // Processing: synthetic ping-pong / dig wave. Listening: real audio-level
    // events only — do not breathe as if the mic is live while WASAPI/WS open.
    if (state !== "listening" && state !== "processing") {
      if (raf.current) cancelAnimationFrame(raf.current);
      raf.current = null;
      listening.current = false;
      hearingRef.current = false;
      return;
    }
    listening.current = state === "listening";
    if (state !== "processing") {
      if (raf.current) cancelAnimationFrame(raf.current);
      raf.current = null;
      return;
    }
    let t0 = performance.now();
    let lastTick = 0;
    const FRAME_MS = 1000 / 50;
    let alive = true;
    const paint = (now: number) => {
      const t = (now - t0) / 1000;
      const next = Array.from({ length: BAR_COUNT }, (_, i) => thinkingBarLevel(t, i));
      smoothed.current = next;
      setLevels(next);
    };
    const tick = (now: number) => {
      if (!alive) return;
      if (now - lastTick < FRAME_MS) {
        raf.current = requestAnimationFrame(tick);
        return;
      }
      lastTick = now;
      paint(now);
      raf.current = requestAnimationFrame(tick);
    };
    paint(t0);
    raf.current = requestAnimationFrame(tick);
    return () => {
      alive = false;
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
      void resizeForState("idle");
    }, LIMIT_MS);
  }

  function clearErrorSoon() {
    if (errorTimer.current) window.clearTimeout(errorTimer.current);
    errorTimer.current = window.setTimeout(() => {
      setState("idle");
      void resizeForState("idle");
    }, ERROR_MS);
  }

  async function openUpgradeSettings() {
    if (limitTimer.current) window.clearTimeout(limitTimer.current);
    if (errorTimer.current) window.clearTimeout(errorTimer.current);
    setState("idle");
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
        listening.current = false;
        hearingRef.current = false;
        setHearing(false);
        smoothed.current = Array(BAR_COUNT).fill(0.14);
        setLevels(Array(BAR_COUNT).fill(0.14));
        if (limitTimer.current) window.clearTimeout(limitTimer.current);
        if (errorTimer.current) window.clearTimeout(errorTimer.current);
        if (!toastActive.current) hidePillSoon();
      } else if (next === "done") {
        listening.current = false;
        // Instant vanish — don't sit on "Done" while history/WAV saves.
        if (!toastActive.current) hidePillSoon();
      } else if (next === "limit") {
        listening.current = false;
        hearingRef.current = false;
        setHearing(false);
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
        listening.current = false;
        hearingRef.current = false;
        setHearing(false);
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
        if (next === "listening") {
          // Connecting only on a fresh session. Replays for a late WebView
          // must not reset live bars back to the dim idle state.
          const alreadyListening = listening.current;
          listening.current = true;
          if (!alreadyListening) {
            hearingRef.current = false;
            setHearing(false);
            smoothed.current = Array(BAR_COUNT).fill(0.1);
            setLevels(Array(BAR_COUNT).fill(0.1));
          }
        } else {
          listening.current = false;
        }
        // Rust owns listening geometry (show_overlay_fast). Re-sizing here races
        // the clear color and flashes white + adds multi-IPC delay.
        void clearOverlayChrome();
      } else {
        setPillLeaving(false);
        void resizeForState(next, toastActive.current);
      }
    }).then((u) => unsubs.push(u));

    listen<string>("dictation-error", () => {
      setState("error");
      void resizeForState("error");
      clearErrorSoon();
    }).then((u) => unsubs.push(u));

    listen("dictation-limit", () => {
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
      // Don't fight the Enhancing… loading wave with stale mic levels.
      if (!listening.current) return;
      if (!hearingRef.current) {
        hearingRef.current = true;
        setHearing(true);
      }
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
        const target = Math.max(0.12, Math.min(0.98, src));
        const prev = smoothed.current[i] ?? 0.14;
        // Rise a bit faster so quiet mics feel responsive; ease down gently.
        const alpha = target > prev ? 0.42 : 0.20;
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
      state === "error" ||
      state === "idle" ||
      pillLeaving);

  const snippet = toast
    ? truncate(toast.original, 36) + " → " + truncate(toast.enhanced, 36)
    : "";

  const connecting = state === "listening" && !hearing;
  const pillActive =
    state === "listening" ||
    state === "processing" ||
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
          className={`liquid-glass-pill flex items-center justify-center px-3.5 py-1 rounded-full select-none ${
            showLimit ? "liquid-glass-pill--limit" : "w-full h-full"
          } ${
            pillLeaving
              ? "liquid-glass-pill--out"
              : state === "idle"
                ? "liquid-glass-pill--idle"
                : connecting
                  ? "liquid-glass-pill--connecting liquid-glass-pill--in"
                  : state === "processing"
                    ? "liquid-glass-pill--thinking liquid-glass-pill--in"
                    : pillActive
                      ? "liquid-glass-pill--in"
                      : ""
          }`}
        >
          {!showLimit && (
            <div
              className="overlay-waveform flex items-end justify-center"
              style={{ gap: `${BAR_GAP_PX}px`, height: `${BAR_MAX_PX}px` }}
            >
              {levels.map((level, i) => {
                const mid =
                  1 -
                  (Math.abs(i - (BAR_COUNT - 1) / 2) / ((BAR_COUNT - 1) / 2)) * 0.18;
                const px = Math.max(3, level * mid * BAR_MAX_PX);
                const isOrange = i % 6 === 3;
                return (
                  <div
                    key={i}
                    className={`liquid-glass-bar shrink-0 origin-bottom ${
                      isOrange ? "liquid-glass-bar--orange" : ""
                    }`}
                    style={{ height: `${px.toFixed(2)}px`, width: `${BAR_WIDTH_PX}px` }}
                  />
                );
              })}
            </div>
          )}

          {showLimit && (
            <span className="text-[10px] font-medium text-white/90 w-full text-center truncate">
              Weekly limit reached
            </span>
          )}
        </div>
      )}
    </div>
  );
}

function pingPong01(t: number, oneWayS: number): number {
  const cycle = oneWayS * 2;
  const x = ((t % cycle) + cycle) % cycle;
  return x < oneWayS ? x / oneWayS : 2 - x / oneWayS;
}

function gaussian(dist: number, sigma: number): number {
  return Math.exp(-(dist * dist) / (2 * sigma * sigma));
}

/** Processing / enhance: traveling ping-pong, then a slower dual-phase “dig”. */
function thinkingBarLevel(t: number, i: number): number {
  const n = BAR_COUNT - 1;
  const pulse = pingPong01(t, 1.05);
  const a1 = 0.14 + gaussian(i - pulse * n, 1.65) * 0.8;

  const t2 = Math.max(0, t - THINKING_DIG_AFTER_S);
  const dig = pingPong01(t2, 1.9);
  const pos = dig * n;
  const wide = gaussian(i - pos, 3.5);
  const counter = gaussian(i - (n - pos), 1.5);
  const floor = 0.13 + 0.05 * (0.5 + 0.5 * Math.sin(t2 * 0.9));
  const a2 = floor + wide * 0.55 + counter * 0.28;

  if (t < THINKING_DIG_AFTER_S) return Math.min(0.98, a1);
  const k = Math.min(1, (t - THINKING_DIG_AFTER_S) / 0.4);
  const eased = k * k * (3 - 2 * k);
  return Math.min(0.98, a1 + (a2 - a1) * eased);
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

/** Click-through goes through Rust: `setIgnoreCursorEvents` adds WS_EX_LAYERED,
 *  which makes WebView2 composite every transparent pixel as opaque white. */
async function setClickThrough(enabled: boolean) {
  try {
    await invoke("set_overlay_click_through", { enabled });
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
  } catch (e) {
    console.error("Failed to position overlay", e);
  }
}

/** Keep the overlay WebView shown (never hide) so Windows doesn't cold-wake it
 *  on hotkey. Idle parks OFF-SCREEN at pill size — avoids a permanent white
 *  strip when DWM fails to composite alpha (same approach as e207754). */
async function resizeForState(state: DictationState, withToast = false) {
  try {
    const win = getCurrentWindow();
    await clearOverlayChrome();
    if (state === "idle" && !withToast) {
      // Rust parks off-screen before any chrome teardown (no white flash).
      await invoke("park_overlay_idle").catch(async () => {
        await Promise.all([
          win.setSize(new LogicalSize(OVERLAY_W, OVERLAY_H)),
          win.setPosition(new LogicalPosition(-40_000, -40_000)),
          win.show(),
        ]);
      });
      await clearOverlayChrome();
      await setClickThrough(true);
      return;
    }
    const needsClicks = state === "limit" || withToast;
    // Toast/limit are larger than the capsule — drop the GDI clip first.
    if (needsClicks) {
      await invoke("set_overlay_pill_clip", { apply: false }).catch(() => {});
    }
    // Geometry first while chrome is cleared; show only after size is ready.
    if (state === "limit") {
      await positionBottomCenter(LIMIT_W, LIMIT_H);
    } else if (withToast) {
      await positionBottomCenter(TOAST_W, TOAST_H);
    } else {
      await positionBottomCenter(OVERLAY_W, OVERLAY_H);
    }
    await win.show();
    await clearOverlayChrome();
    await setClickThrough(!needsClicks);
  } catch {
    // ignore
  }
}
