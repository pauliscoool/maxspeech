import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface TranscriptionResult {
  text: string;
  speakers: { speaker: string; text: string }[];
  duration_secs: number;
}

export default function Transcriber() {
  const [file, setFile] = useState<string | null>(null);
  const [result, setResult] = useState<TranscriptionResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    const files = e.dataTransfer.files;
    if (files.length > 0) {
      const path = (files[0] as unknown as { path?: string }).path;
      if (path) {
        setFile(path);
        transcribe(path);
      }
    }
  }

  async function transcribe(path: string) {
    setLoading(true);
    setError("");
    setResult(null);
    try {
      const res = await invoke<TranscriptionResult>("transcribe_file", { path });
      setResult(res);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function exportAs(format: "txt" | "srt" | "md") {
    if (!result) return;
    await invoke("export_transcription", { format, text: result.text });
  }

  return (
    <div className="flex flex-col h-screen bg-[var(--ms-bg)]">
      <header className="p-4 border-b border-[var(--ms-border)]">
        <h1 className="text-lg font-semibold">Audio Transcriber</h1>
      </header>
      <main className="flex-1 p-6 overflow-y-auto">
        {!result && !loading && (
          <div
            onDrop={handleDrop}
            onDragOver={(e) => e.preventDefault()}
            className="flex flex-col items-center justify-center h-64 border-2 border-dashed border-[var(--ms-border)] rounded-xl hover:border-indigo-500 transition-colors cursor-pointer"
          >
            <p className="text-[var(--ms-text-dim)] text-sm">
              {file ? file : "Drop an audio or video file here"}
            </p>
          </div>
        )}
        {loading && (
          <div className="flex items-center justify-center h-64">
            <div className="animate-pulse text-indigo-400 text-sm">Transcribing...</div>
          </div>
        )}
        {error && (
          <p className="text-red-400 text-sm">{error}</p>
        )}
        {result && (
          <div className="space-y-4">
            <div className="flex gap-2">
              <button onClick={() => exportAs("txt")} className="px-3 py-1 rounded bg-[var(--ms-surface)] border border-[var(--ms-border)] text-xs hover:border-indigo-500">.txt</button>
              <button onClick={() => exportAs("srt")} className="px-3 py-1 rounded bg-[var(--ms-surface)] border border-[var(--ms-border)] text-xs hover:border-indigo-500">.srt</button>
              <button onClick={() => exportAs("md")} className="px-3 py-1 rounded bg-[var(--ms-surface)] border border-[var(--ms-border)] text-xs hover:border-indigo-500">.md</button>
            </div>
            <div className="p-4 rounded-lg bg-[var(--ms-surface)] border border-[var(--ms-border)] text-sm whitespace-pre-wrap">
              {result.text}
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
