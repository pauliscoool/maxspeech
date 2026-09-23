import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import AppleSpinner from "../../components/AppleSpinner";

interface TranscriptionResult {
  text: string;
  speakers: { speaker: string; text: string }[];
  duration_secs: number;
}

export default function TranscriberPage() {
  const [file, setFile] = useState<string | null>(null);
  const [result, setResult] = useState<TranscriptionResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [dragOver, setDragOver] = useState(false);
  // Tauri's HTML5 drag events never carry a real filesystem path (that's a
  // browser-only File API detail Tauri doesn't fill in) — the webview's own
  // drag-drop event is the only source of an absolute path to hand Rust.
  const loadingRef = useRef(loading);
  loadingRef.current = loading;
  const resultRef = useRef(result);
  resultRef.current = result;

  async function transcribe(path: string) {
    setFile(path);
    setLoading(true);
    setError("");
    setResult(null);
    try {
      setResult(await invoke<TranscriptionResult>("transcribe_file", { path }));
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "enter" || payload.type === "over") {
          if (!loadingRef.current && !resultRef.current) setDragOver(true);
          return;
        }
        if (payload.type === "leave") {
          setDragOver(false);
          return;
        }
        if (payload.type === "drop") {
          setDragOver(false);
          if (loadingRef.current || resultRef.current) return;
          const path = payload.paths[0];
          if (!path) {
            setError("Could not read file path.");
            return;
          }
          void transcribe(path);
        }
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => unlisten?.();
  }, []);

  async function exportAs(format: "txt" | "srt" | "md") {
    if (!result) return;
    await invoke("export_transcription", { format, text: result.text });
  }

  return (
    <div className="page-shell space-y-5">
      <header>
        <h1 className="page-title">Audio Transcriber</h1>
        <p className="page-subtitle">
          Drop a meeting or lecture file — get a transcript with speaker labels.
        </p>
      </header>

      {!result && !loading && (
        <div
          className="flex flex-col items-center justify-center h-44 sm:h-52 rounded-2xl border border-dashed transition-colors bg-[var(--ms-surface)]"
          style={{
            borderColor: dragOver
              ? "var(--ms-turquoise)"
              : "color-mix(in srgb, var(--ms-text-dim) 35%, transparent)",
          }}
        >
          <p className="text-sm text-[var(--ms-text-dim)] px-4 text-center truncate max-w-full">
            {file || "Drop audio or video here"}
          </p>
        </div>
      )}

      {loading && (
        <div className="surface-card p-10 flex flex-col items-center justify-center gap-3">
          <AppleSpinner size={26} />
          <p className="text-sm text-[var(--ms-text-dim)]">Transcribing…</p>
        </div>
      )}

      {error && <p className="text-sm text-[var(--ms-error)]">{error}</p>}

      {result && (
        <div className="space-y-4">
          <div className="flex flex-wrap gap-2">
            {(["txt", "srt", "md"] as const).map((fmt) => (
              <button
                key={fmt}
                onClick={() => exportAs(fmt)}
                className="px-3 py-1.5 rounded-full text-xs hover:bg-[var(--ms-turquoise-glow)] hover:text-[var(--ms-turquoise)] transition-colors"
                style={{ background: "var(--ms-fill-muted)" }}
              >
                Export .{fmt}
              </button>
            ))}
            <button
              onClick={() => {
                setResult(null);
                setFile(null);
              }}
              className="px-3 py-1.5 rounded-full text-xs text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)]"
            >
              New file
            </button>
          </div>
          <div className="surface-card p-4 sm:p-5 text-sm whitespace-pre-wrap leading-relaxed break-words">
            {result.text}
          </div>
        </div>
      )}
    </div>
  );
}
