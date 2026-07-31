import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import {
  getCurrentWindow,
  currentMonitor,
  LogicalSize,
  LogicalPosition,
} from "@tauri-apps/api/window";

type DictationState = "idle" | "listening" | "processing" | "done" | "error";

interface TranscriptEvent {
  text: string;
  is_final: boolean;
}

interface EnhanceEvent {
  original: string;
  enhanced: string;
}

const BAR_COUNT = 29;
const BAR_MAX_PX = 18;
const OVERLAY_W = 218;
const OVERLAY_H = 38;
const TOAST_W = 275;
const TOAST_H = 95;
const TOAST_MS = 3000;

export default function Overlay() {
  const [state, setState] = useState<DictationState>("idle");
  const [transcript, setTranscript] = useState("");
  const [error, setError] = useState("");
  const [levels, setLevels] = useState<number[]>(() => Array(BAR_COUNT).fill(0.12));
  const [showLive, setShowLive] = useState(true);
  const [toast, setToast] = useState<EnhanceEvent | null>(null);
  const [toastLeaving, setToastLeaving] = useState(false);
  const smoothed = useRef<number[]>(Array(BAR_COUNT).fill(0.12));
  const raf = useRef<number | null>(null);
  const listening = useRef(false);
  const toastTimer = useRef<number | null>(null);
  const toastActive = useRef(false);

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
    const FRAME_MS = 1000 / 30; // 30fps is plenty for a decorative idle wave
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
      if (avg < 0.22) {
        const next = smoothed.current.map((v, i) => {
          const wave =
            0.1 +
            0.18 * (0.5 + 0.5 * Math.sin(t * 7.5 + i * 0.55)) *
              (0.55 + 0.45 * Math.sin(t * 3.2 + i * 0.9));
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
      }, 280);
    }, TOAST_MS);
  }

  useEffect(() => {
    const unsubs: (() => void)[] = [];

    listen<string>("dictation-state", (e) => {
      const next = e.payload as DictationState;
      setState(next);
      if (next === "idle") {
        setTranscript("");
        setError("");
        smoothed.current = Array(BAR_COUNT).fill(0.12);
        setLevels(Array(BAR_COUNT).fill(0.12));
        if (!toastActive.current) void resizeForState("idle");
      } else if (next === "listening" || next === "processing") {
        setToast(null);
        setToastLeaving(false);
        toastActive.current = false;
        if (toastTimer.current) window.clearTimeout(toastTimer.current);
        void resizeForState(next);
      } else {
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
        const target = Math.max(0.06, Math.min(1, Number(incoming[i] ?? 0.06)));
        const prev = smoothed.current[i] ?? 0.12;
        const alpha = target > prev ? 0.72 : 0.38;
        const v = prev + (target - prev) * alpha;
        smoothed.current[i] = v;
        return v;
      });
      setLevels(next);
    }).then((u) => unsubs.push(u));

    return () => {
      unsubs.forEach((u) => u());
      if (toastTimer.current) window.clearTimeout(toastTimer.current);
    };
  }, []);

  const showToast = !!toast;
  if (state === "idle" && !showToast) {
    return <div className="overlay-root" />;
  }

  const statusLabel =
    state === "listening"
      ? "Listening…"
      : state === "processing"
        ? "Enhancing…"
        : state === "done"
          ? "Done"
          : "Error";

  const label = error || (showLive && transcript ? transcript : statusLabel);

  const snippet = toast
    ? truncate(toast.original, 36) + " → " + truncate(toast.enhanced, 36)
    : "";

  return (
    <div className={`overlay-root ${showToast ? "overlay-root--toast" : ""}`}>
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

      {!showToast && (
        <div
          data-tauri-drag-region
          className="liquid-glass-pill flex items-center gap-1.5 px-2.5 py-1 rounded-full select-none w-full h-full"
        >
          <div className="flex items-end justify-center gap-[1.5px] h-[18px] flex-1 min-w-[94px]">
            {levels.map((level, i) => {
              const mid =
                1 -
                (Math.abs(i - (BAR_COUNT - 1) / 2) / ((BAR_COUNT - 1) / 2)) * 0.18;
              const px = Math.max(2, Math.round(level * mid * BAR_MAX_PX));
              const isOrange = i % 6 === 3;
              return (
                <div
                  key={i}
                  className="liquid-glass-bar shrink-0 origin-bottom"
                  style={{
                    height: `${px}px`,
                    width: "2px",
                    ["--bar-color" as string]: isOrange
                      ? "var(--ms-orange)"
                      : "var(--ms-turquoise)",
                    ["--bar-hi" as string]: isOrange ? "#fdba74" : "#99f6e4",
                  }}
                />
              );
            })}
          </div>

          <span className="text-[8px] text-white/85 truncate max-w-[86px] font-medium">
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
    if (state === "idle" && !withToast) {
      await win.hide();
      return;
    }
    await win.show();
    if (withToast) {
      await positionBottomCenter(TOAST_W, TOAST_H);
    } else {
      await positionBottomCenter(OVERLAY_W, OVERLAY_H);
    }
  } catch {
    // ignore
  }
}
