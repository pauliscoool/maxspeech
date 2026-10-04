import { useCallback, useEffect, useRef, useState, type ReactElement } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Briefcase,
  Check,
  ChevronDown,
  CircleAlert,
  Coffee,
  Feather,
  Landmark,
  Replace,
  Sparkles,
  Square,
  X,
  type LucideIcon,
} from "lucide-react";
import { loadAndApplyTheme } from "../lib/theme";
import { ClaudeIcon, CursorCodexIcon } from "./brandIcons";

type Formality = "casual" | "neutral" | "professional" | "formal" | "claude_code" | "cursor_codex";
type BrandIcon = (props: { size?: number; className?: string }) => ReactElement;
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

interface Option {
  id: Formality;
  label: string;
  hint: string;
  Icon?: LucideIcon;
  Brand?: BrandIcon;
  brand?: "claude" | "duo";
}

const FORMALITIES: Option[] = [
  { id: "casual", label: "Casual", hint: "Relaxed, chatty", Icon: Coffee },
  { id: "neutral", label: "Neutral", hint: "Your voice, cleaned up", Icon: Feather },
  { id: "professional", label: "Professional", hint: "Polished work email", Icon: Briefcase },
  { id: "formal", label: "Formal", hint: "No contractions", Icon: Landmark },
];

const AGENT_PROMPTS: Option[] = [
  {
    id: "claude_code",
    label: "Claude Code",
    hint: "Optimized coding prompt",
    Brand: ClaudeIcon,
    brand: "claude",
  },
  {
    id: "cursor_codex",
    label: "Cursor / Codex",
    hint: "Goal, context, done-when",
    Brand: CursorCodexIcon,
    brand: "duo",
  },
];

const ALL_OPTIONS = [...FORMALITIES, ...AGENT_PROMPTS];

function OptionIcon({ opt, size }: { opt: Option; size: number }) {
  const { Icon, Brand } = opt;
  return (
    <span className={`enh-ico${opt.brand ? ` is-${opt.brand}` : ""}`}>
      {Brand ? (
        <Brand size={size} />
      ) : (
        Icon && <Icon size={size} strokeWidth={2} aria-hidden="true" />
      )}
    </span>
  );
}

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
    void loadAndApplyTheme();
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
  const current = ALL_OPTIONS.find((f) => f.id === formality) ?? FORMALITIES[1];

  function renderItem(f: Option) {
    return (
      <button
        key={f.id}
        type="button"
        role="option"
        aria-selected={f.id === formality}
        className={`enh-menu-item${f.id === formality ? " is-active" : ""}`}
        onClick={() => pickFormality(f.id)}
      >
        <OptionIcon opt={f} size={16} />
        <span className="enh-menu-text">
          <span className="enh-menu-label">{f.label}</span>
          <span className="enh-menu-hint">{f.hint}</span>
        </span>
        {f.id === formality && (
          <Check size={15} strokeWidth={2.4} className="enh-menu-check" aria-hidden="true" />
        )}
      </button>
    );
  }

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
            <OptionIcon opt={current} size={15} />
            <span>{current.label}</span>
            <ChevronDown
              size={14}
              strokeWidth={2.2}
              aria-hidden="true"
              className={`enh-chevron${menuOpen ? " is-open" : ""}`}
            />
          </button>
          {menuOpen && (
            <div className="enh-menu" role="listbox">
              {FORMALITIES.map(renderItem)}
              <div className="enh-menu-group" role="presentation">
                For coding agents
              </div>
              {AGENT_PROMPTS.map(renderItem)}
            </div>
          )}
        </div>
        <span
          className={`enh-status${streaming || phase === "loading" ? " is-live" : ""}`}
          data-tauri-drag-region
        >
          {phase === "error" ? (
            <CircleAlert size={13} strokeWidth={2.2} aria-hidden="true" />
          ) : (
            <Sparkles size={13} strokeWidth={2.2} aria-hidden="true" />
          )}
          {phase === "loading" &&"Reading your text…"}
          {streaming && "Enhancing…"}
          {phase === "done" && "Ready"}
          {phase === "stopped" && "Stopped"}
          {phase === "error" && "Error"}
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
          <X size={15} strokeWidth={2.2} aria-hidden="true" />
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
            <Square size={11} strokeWidth={0} fill="currentColor" className="enh-stop-icon" aria-hidden="true" />
            Stop
          </button>
          <button
            type="button"
            className={`enh-btn enh-btn-replace${canReplace ? "" : " is-hidden"}`}
            onClick={replace}
            disabled={!canReplace}
            tabIndex={canReplace ? 0 : -1}
          >
            <Replace size={15} strokeWidth={2.2} aria-hidden="true" />
            Replace
          </button>
        </div>
      </div>
    </div>
  );
}
