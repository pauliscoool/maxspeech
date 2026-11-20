import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Check, X } from "lucide-react";

interface Suggestion {
  id: number;
  kind: "word" | "grammar";
  original: string;
  replacement: string;
}

const AUTO_HIDE_MS = 7000;

export default function AutocorrectChip() {
  const [s, setS] = useState<Suggestion | null>(null);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    const offShow = listen<Suggestion>("autocorrect-chip-show", (ev) => {
      setS(ev.payload);
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => {
        void invoke("autocorrect_dismiss", { id: ev.payload.id, ignore: false });
      }, AUTO_HIDE_MS);
    });
    const offHide = listen("autocorrect-chip-hide", () => {
      window.clearTimeout(timer.current);
      setS(null);
    });
    return () => {
      window.clearTimeout(timer.current);
      void offShow.then((f) => f());
      void offHide.then((f) => f());
    };
  }, []);

  if (!s) return <div className="ac-chip" />;

  const shown = s.replacement === "" ? `remove "${s.original}"` : s.replacement;

  return (
    <div className="ac-chip" role="group" aria-label="Autocorrect suggestion">
      <button
        className="ac-chip-main"
        onClick={() => void invoke("autocorrect_accept", { id: s.id })}
        title="Apply this fix"
      >
        <Check size={14} strokeWidth={2.5} aria-hidden="true" />
        <span className="ac-chip-from">{s.original}</span>
        <span className="ac-chip-arrow">→</span>
        <span className="ac-chip-to">{shown}</span>
      </button>
      {s.kind === "word" && (
        <button
          className="ac-chip-link"
          onClick={() => void invoke("autocorrect_dismiss", { id: s.id, ignore: true })}
          title="Never change this word"
        >
          Never
        </button>
      )}
      <button
        className="ac-chip-x"
        onClick={() => void invoke("autocorrect_dismiss", { id: s.id, ignore: false })}
        aria-label="Dismiss"
      >
        <X size={14} aria-hidden="true" />
      </button>
    </div>
  );
}
