import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Toggle from "../../components/Toggle";

interface Status {
  enabled: boolean;
  ready: boolean;
  fixed: number;
  suggested: number;
  keys: number;
  note: string;
}

const EXCLUDED_KEY = "autocorrect_excluded_apps";

export default function AutocorrectPage() {
  const [enabled, setEnabled] = useState(false);
  const [grammar, setGrammar] = useState(true);
  const [status, setStatus] = useState<Status | null>(null);
  const [ignored, setIgnored] = useState<string[]>([]);
  const [excluded, setExcluded] = useState<string[]>([]);
  const [ignoreInput, setIgnoreInput] = useState("");
  const [appInput, setAppInput] = useState("");
  const [error, setError] = useState("");

  async function loadStatus() {
    try {
      setStatus(await invoke<Status>("get_autocorrect_status"));
    } catch {
      setStatus(null);
    }
  }

  async function loadLists() {
    try {
      setIgnored(await invoke<string[]>("get_autocorrect_ignore"));
      setExcluded(await invoke<string[]>("get_autocorrect_excluded"));
    } catch {
      setIgnored([]);
      setExcluded([]);
    }
  }

  useEffect(() => {
    void invoke<string>("get_setting", { key: "autocorrect_enabled" }).then((v) =>
      setEnabled(v === "true"),
    );
    void invoke<string>("get_setting", { key: "autocorrect_grammar" }).then((v) =>
      setGrammar(v !== "false"),
    );
    void loadLists();
    void loadStatus();
    const timer = window.setInterval(() => void loadStatus(), 2000);
    return () => window.clearInterval(timer);
  }, []);

  async function toggle() {
    const next = !enabled;
    setEnabled(next);
    try {
      await invoke("set_setting", { key: "autocorrect_enabled", value: String(next) });
      setError("");
    } catch (e) {
      setEnabled(!next);
      setError(String(e));
    }
    window.setTimeout(() => void loadStatus(), 600);
  }

  async function toggleGrammar() {
    const next = !grammar;
    setGrammar(next);
    try {
      await invoke("set_setting", { key: "autocorrect_grammar", value: String(next) });
    } catch (e) {
      setGrammar(!next);
      setError(String(e));
    }
  }

  async function addIgnored() {
    const w = ignoreInput.trim();
    if (!w) return;
    try {
      await invoke("add_autocorrect_ignore", { word: w });
      setIgnoreInput("");
      await loadLists();
    } catch (e) {
      setError(String(e));
    }
  }

  async function removeIgnored(word: string) {
    await invoke("delete_autocorrect_ignore", { word });
    await loadLists();
  }

  async function saveExcluded(next: string[]) {
    await invoke("set_setting", { key: EXCLUDED_KEY, value: next.join("\n") });
    setExcluded(next);
  }

  async function addApp() {
    const exe = appInput.trim().toLowerCase();
    if (!exe) return;
    const name = exe.endsWith(".exe") ? exe : `${exe}.exe`;
    if (!excluded.includes(name)) await saveExcluded([...excluded, name].sort());
    setAppInput("");
  }

  return (
    <div className="page-shell space-y-5">
      <header>
        <h1 className="page-title">Autocorrect</h1>
        <p className="page-subtitle">
          Fixes typos as you type in any app. Runs entirely on this PC.
        </p>
      </header>

      <div className="surface-card p-4 space-y-3">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="text-sm font-medium">Fix typos while I type</div>
            <div className="text-xs text-[var(--ms-text-dim)] mt-0.5">
              Corrects a word when you press space. Press Backspace right after to undo a fix.
            </div>
          </div>
          <Toggle checked={enabled} onChange={toggle} label="Fix typos while I type" />
        </div>
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="text-sm font-medium">Suggest grammar fixes</div>
            <div className="text-xs text-[var(--ms-text-dim)] mt-0.5">
              Shows a small chip after a sentence for things like “she go” or “an test”. Never
              changes anything until you click it.
            </div>
          </div>
          <Toggle checked={grammar} onChange={toggleGrammar} label="Suggest grammar fixes" />
        </div>
        {enabled && status && (
          <div className="text-xs text-[var(--ms-text-dim)]">
            {status.ready ? "Ready" : "Loading dictionary…"} · {status.keys} letters seen ·{" "}
            {status.fixed} fixed this session
            {status.note && <div className="mt-0.5">Last app: {status.note}</div>}
          </div>
        )}
        {error && <div className="text-xs text-[var(--ms-error)]">{error}</div>}
        <p className="text-xs text-[var(--ms-text-dim)]">
          Keystrokes are read only to spot misspelled words. Nothing you type is stored, logged, or
          sent anywhere, and password fields, terminals, and code editors are skipped. Dictated text
          is never changed.
        </p>
      </div>

      <section className="space-y-2">
        <h2 className="text-sm font-medium">Never change these words</h2>
        <div className="flex gap-2">
          <input
            value={ignoreInput}
            onChange={(e) => setIgnoreInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void addIgnored()}
            placeholder="Add a word…"
            className="input-field flex-1 min-w-0 px-3 py-2.5 text-sm"
          />
          <button
            onClick={() => void addIgnored()}
            disabled={!ignoreInput.trim()}
            className="btn-primary px-4 py-2.5 text-sm shrink-0"
          >
            Add
          </button>
        </div>
        <p className="text-xs text-[var(--ms-text-dim)]">
          Words in your Dictionary are also left alone.
        </p>
        {ignored.map((w) => (
          <div key={w} className="surface-card px-4 py-3 flex items-center justify-between gap-3">
            <span className="text-sm truncate">{w}</span>
            <button
              onClick={() => void removeIgnored(w)}
              className="text-xs text-[var(--ms-text-dim)] hover:text-[var(--ms-error)] transition-colors shrink-0"
            >
              Remove
            </button>
          </div>
        ))}
      </section>

      <section className="space-y-2">
        <h2 className="text-sm font-medium">Turned off in these apps</h2>
        <div className="flex gap-2">
          <input
            value={appInput}
            onChange={(e) => setAppInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void addApp()}
            placeholder="app name, e.g. slack.exe"
            className="input-field flex-1 min-w-0 px-3 py-2.5 text-sm"
          />
          <button
            onClick={() => void addApp()}
            disabled={!appInput.trim()}
            className="btn-primary px-4 py-2.5 text-sm shrink-0"
          >
            Add
          </button>
        </div>
        <div className="flex flex-wrap gap-2">
          {excluded.map((exe) => (
            <button
              key={exe}
              onClick={() => void saveExcluded(excluded.filter((e) => e !== exe))}
              title="Click to turn autocorrect back on for this app"
              className="surface-card px-3 py-1.5 text-xs hover:text-[var(--ms-error)] transition-colors"
            >
              {exe} ✕
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}
