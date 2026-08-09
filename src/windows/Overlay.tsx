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

/** Fewer, thicker bars — reads louder than the old skinny strip. */
const BAR_COUNT = 21;
const BAR_MAX_PX = 30;
const BAR_WIDTH_PX = 3;
/** Tighter sides than the old 218px pill; taller so the waveform can jump. */
const OVERLAY_W = 158;
const OVERLAY_H = 52;
const PILL_OUT_MS = 80;
const TOAST_W = 220;
const TOAST_H = 90;
const TOAST_MS = 1600;
const TOAST_OUT_MS = 120;
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
    setPillLeaving(true);
    pillOutTimer.current = window.setTimeout(() => {
      setPillLeaving(false);
      void resizeForState("idle");
    }, PILL_OUT_MS);
  }

  useEffect(() => {
    positionBottomCenter(OVERLAY_W, OVERLAY_H);
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
    const FRAME_MS = 1000 / 60;
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
          const wave =
            0.14 +
            0.28 * (0.5 + 0.5 * Math.sin(t * 9.5 + i * 0.62)) *
              (0.55 + 0.45 * Math.sin(t * 4.1 + i * 0.85));
          return Math.max(v, wave);
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
      await invoke("open_settings_page");
    } catch (e) {
      console.error("Failed to open settings", e);
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
        setToast(null);
        setToastLeaving(false);
        setPillLeaving(false);
        toastActive.current = false;
        if (toastTimer.current) window.clearTimeout(toastTimer.current);
        if (pillOutTimer.current) window.clearTimeout(pillOutTimer.current);
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
        const target = Math.max(0.08, Math.min(1, src));
        const prev = smoothed.current[i] ?? 0.14;
        // Snappy rise / quick fall so the pill doesn't feel laggy.
        const alpha = target > prev ? 0.9 : 0.55;
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
  const showPill =
    showLimit ||
    state === "listening" ||
    state === "processing" ||
    state === "done" ||
    state === "error" ||
    pillLeaving;

  if (state === "idle" && !showToast && !pillLeaving) {
    return <div className="overlay-root" />;
  }

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
              : "";

  const label =
    state === "limit"
      ? "Weekly limit reached"
      : error || (showLive && transcript ? transcript : statusLabel);

  const snippet = toast
    ? truncate(toast.original, 36) + " → " + truncate(toast.enhanced, 36)
    : "";

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
            <div className="enhance-toast-sub">Open Settings to upgrade your plan</div>
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

      {!showToast && showPill && (
        <div
          data-tauri-drag-region
          className={`liquid-glass-pill flex items-center gap-1.5 px-2.5 py-1.5 rounded-full select-none ${
            showLimit ? "liquid-glass-pill--limit" : "w-full h-full"
          } ${pillLeaving ? "liquid-glass-pill--out" : "liquid-glass-pill--in"}`}
        >
          {!showLimit && (
            <div
              className="flex items-end justify-center flex-1 min-w-0"
              style={{ gap: "2px", height: `${BAR_MAX_PX}px` }}
            >
              {levels.map((level, i) => {
                const mid =
                  1 -
                  (Math.abs(i - (BAR_COUNT - 1) / 2) / ((BAR_COUNT - 1) / 2)) * 0.22;
                const px = Math.max(3, Math.round(level * mid * BAR_MAX_PX));
                const isOrange = i % 5 === 2;
                return (
                  <div
                    key={i}
                    className="liquid-glass-bar shrink-0 origin-bottom"
                    style={{
                      height: `${px}px`,
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
                : "text-[9px] text-white/90 max-w-[56px]"
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

async function positionBottomCenter(w: number, h: number) {
  try {
    const win = getCurrentWindow();
    const monitor = await currentMonitor();
    if (!monitor) return;
    const scale = monitor.scaleFactor;
    const screenW = monitor.size.width / scale;
    const screenH = monitor.size.height / scale;
    const clear = [0, 0, 0, 0] as [number, number, number, number];
    await win.setBackgroundColor(clear).catch(() => {});
    await win.setSize(new LogicalSize(w, h));
    await win.setPosition(
      new LogicalPosition((screenW - w) / 2, screenH - h - 48),
    );
    await win.setAlwaysOnTop(true);
  } catch (e) {
    console.error("Failed to position overlay", e);
  }
}

async function resizeForState(state: DictationState, withToast = false) {
  try {
    const win = getCurrentWindow();
    const clear = [0, 0, 0, 0] as [number, number, number, number];
    await win.setBackgroundColor(clear).catch(() => {});
    if (state === "idle" && !withToast) {
      await win.hide();
      return;
    }
    await win.show();
    if (state === "limit") {
      await positionBottomCenter(LIMIT_W, LIMIT_H);
    } else if (withToast) {
      await positionBottomCenter(TOAST_W, TOAST_H);
    } else {
      await positionBottomCenter(OVERLAY_W, OVERLAY_H);
    }
  } catch {
    // ignore
  }
}
