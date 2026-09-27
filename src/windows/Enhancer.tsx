import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type Formality = "casual" | "neutral" | "professional" | "formal";
type Phase = "loading" | "streaming" | "done" | "stopped" | "error";

interface SessionInfo {
  original: string;
  formality: Formality;
  error: string | null;
}

interface UpdateEvent {
  run: number;
  text: string;
  state: "streaming" | "done" | "stopped" | "error";
  error: string | null;
}

const FORMALITIES: { id: Formality; label: string; hint: string }[] = [
  { id: "casual", label: "Casual", hint: "Relaxed, chatty" },
  { id: "neutral", label: "Neutral", hint: "Your voice, cleaned up" },
  { id: "professional", label: "Professional", hint: "Polished work email" },
  { id: "formal", label: "Formal", hint: "No contractions" },
];

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export default function Enhancer() {
  const [phase, setPhase] = useState<Phase>("loading");
  const [formality, setFormality] = useState<Formality>("neutral");
  const [menuOpen, setMenuOpen] = useState(false);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const highestRun = useRef(0);
  const minRun = useRef(0);
  const bodyRef = useRef<HTMLDivElement | null>(null);

  const start = useCallback((f: Formality) => {
    setPhase("streaming");
    setText("");
    setError(null);
    setMenuOpen(false);
    minRun.current = highestRun.current + 1;
    invoke<number>("enhancer_run", { formality: f })
      .then((run) => {
        minRun.current = run;
      })
      .catch((e) => {
        setPhase("error");
        setError(errText(e));
      });
  }, []);

  const loadSession = useCallback(() => {
    setPhase("loading");
    setText("");
    setError(null);
    setMenuOpen(false);
    invoke<SessionInfo>("enhancer_session")
      .then((s) => {
        setFormality(s.formality);
        if (s.error) {
          setPhase("error");
          setError(s.error);
          return;
        }
        start(s.formality);
      })
      .catch((e) => {
        setPhase("error");
        setError(errText(e));
      });
  }, [start]);

  useEffect(() => {
    loadSession();
    const unlistenOpen = listen("enhancer:open", loadSession);
    const unlistenUpdate = listen<UpdateEvent>("enhancer:update", (ev) => {
      const u = ev.payload;
      highestRun.current = Math.max(highestRun.current, u.run);
      if (u.run < minRun.current) return;
      setText(u.text);
      setPhase(u.state);
      setError(u.error);
    });
    return () => {
      void unlistenOpen.then((fn) => fn());
      void unlistenUpdate.then((fn) => fn());
    };
  }, [loadSession]);

  useEffect(() => {
    const el = bodyRef.current;
    if (el && phase === "streaming") el.scrollTop = el.scrollHeight;
  }, [text, phase]);

  function pickFormality(f: Formality) {
    setFormality(f);
    start(f);
  }

  function replace() {
    invoke("enhancer_replace").catch((e) => {
      setPhase("error");
      setError(errText(e));
    });
  }

  const streaming = phase === "streaming";
  const canReplace = (phase === "done" || phase === "stopped") && text.trim() !== "";
  const current = FORMALITIES.find((f) => f.id === formality) ?? FORMALITIES[1];

  return (
    <div className="enh-root">
      <div className="enh-header" data-tauri-drag-region>
        <div className="enh-select">
          <button
            type="button"
            className="enh-select-btn"
            onClick={() => setMenuOpen((o) => !o)}
            aria-haspopup="listbox"
            aria-expanded={menuOpen}
          >
            <span>{current.label}</span>
            <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
              <path
                d="M2.5 4.5 6 8l3.5-3.5"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.6"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
          </button>
          {menuOpen && (
            <div className="enh-menu" role="listbox">
              {FORMALITIES.map((f) => (
                <button
                  key={f.id}
                  type="button"
                  role="option"
                  aria-selected={f.id === formality}
                  className={`enh-menu-item${f.id === formality ? " is-active" : ""}`}
                  onClick={() => pickFormality(f.id)}
                >
                  <span className="enh-menu-label">{f.label}</span>
                  <span className="enh-menu-hint">{f.hint}</span>
                </button>
              ))}
            </div>
          )}
        </div>
        <span className="enh-status" data-tauri-drag-region>
          {phase === "loading" && "Reading your text…"}
          {streaming && "Enhancing…"}
          {phase === "done" && "Ready"}
          {phase === "stopped" && "Stopped"}
        </span>
      </div>

      <div className="enh-body" ref={bodyRef} onClick={() => setMenuOpen(false)}>
        {phase === "error" && !text ? (
          <p className="enh-error">{error ?? "Something went wrong."}</p>
        ) : (
          <p className="enh-text">
            {text}
            {streaming && <span className="enh-caret" />}
          </p>
        )}
        {phase === "error" && text && <p className="enh-error">{error}</p>}
      </div>

      <div className="enh-footer">
        <button type="button" className="enh-btn enh-btn-ghost" onClick={() => void invoke("enhancer_close")}>
          Close
        </button>
        <div className="enh-action">
          <button
            type="button"
            className={`enh-btn enh-btn-stop${streaming ? "" : " is-hidden"}`}
            onClick={() => void invoke("enhancer_stop")}
            disabled={!streaming}
            tabIndex={streaming ? 0 : -1}
          >
            <span className="enh-stop-icon" />
            Stop
          </button>
          <button
            type="button"
            className={`enh-btn enh-btn-replace${canReplace ? "" : " is-hidden"}`}
            onClick={replace}
            disabled={!canReplace}
            tabIndex={canReplace ? 0 : -1}
          >
            Replace
          </button>
        </div>
      </div>
    </div>
  );
}
