import { useState, useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

type DictationState = "idle" | "listening" | "processing" | "done" | "error";

interface TranscriptEvent {
  text: string;
  is_final: boolean;
}

export default function Overlay() {
  const [state, setState] = useState<DictationState>("idle");
  const [transcript, setTranscript] = useState("");
  const [error, setError] = useState("");
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animRef = useRef<number>(0);

  useEffect(() => {
    const unsubs: (() => void)[] = [];
    listen<string>("dictation-state", (e) => {
      setState(e.payload as DictationState);
      if (e.payload === "idle") {
        setTranscript("");
        setError("");
      }
    }).then((u) => unsubs.push(u));

    listen<TranscriptEvent>("transcript", (e) => {
      setTranscript(e.payload.text);
    }).then((u) => unsubs.push(u));

    listen<string>("dictation-error", (e) => {
      setError(e.payload);
      setState("error");
    }).then((u) => unsubs.push(u));

    listen<Float32Array>("audio-level", (e) => {
      drawWaveform(e.payload);
    }).then((u) => unsubs.push(u));

    return () => unsubs.forEach((u) => u());
  }, []);

  function drawWaveform(levels: Float32Array | number[]) {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const w = canvas.width;
    const h = canvas.height;
    ctx.clearRect(0, 0, w, h);
    ctx.fillStyle = "#6366f1";
    const barCount = Math.min(levels.length, 32);
    const barW = w / barCount - 1;
    for (let i = 0; i < barCount; i++) {
      const val = Math.min(1, Math.abs(Number(levels[i]) || 0));
      const barH = Math.max(2, val * h);
      ctx.fillRect(i * (barW + 1), (h - barH) / 2, barW, barH);
    }
  }

  useEffect(() => {
    if (state === "listening") {
      const pulse = () => {
        animRef.current = requestAnimationFrame(pulse);
      };
      pulse();
      return () => cancelAnimationFrame(animRef.current);
    }
  }, [state]);

  if (state === "idle") return null;

  const stateColors: Record<DictationState, string> = {
    idle: "",
    listening: "border-indigo-500 shadow-indigo-500/30",
    processing: "border-amber-500 shadow-amber-500/30",
    done: "border-emerald-500 shadow-emerald-500/30",
    error: "border-red-500 shadow-red-500/30",
  };

  const stateLabels: Record<DictationState, string> = {
    idle: "",
    listening: "Listening...",
    processing: "Processing...",
    done: "Done",
    error: "Error",
  };

  return (
    <div
      data-tauri-drag-region
      className="flex items-center gap-3 px-4 py-2 rounded-full border shadow-lg backdrop-blur-md bg-[var(--ms-surface)]/90 select-none"
      style={{ borderColor: "inherit" }}
    >
      <div
        className={`flex items-center gap-3 rounded-full ${stateColors[state]}`}
      >
        <div
          className={`w-2.5 h-2.5 rounded-full ${
            state === "listening"
              ? "bg-indigo-500 animate-pulse"
              : state === "processing"
                ? "bg-amber-500 animate-pulse"
                : state === "done"
                  ? "bg-emerald-500"
                  : "bg-red-500"
          }`}
        />
        {state === "listening" && (
          <canvas ref={canvasRef} width={120} height={24} className="opacity-80" />
        )}
        <span className="text-sm text-[var(--ms-text-dim)]">
          {error || transcript || stateLabels[state]}
        </span>
      </div>
    </div>
  );
}
