import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Macro {
  id: number;
  trigger: string;
  expansion: string;
}

export default function SnippetsPage() {
  const [macros, setMacros] = useState<Macro[]>([]);
  const [trigger, setTrigger] = useState("");
  const [expansion, setExpansion] = useState("");

  async function load() {
    try {
      setMacros(await invoke<Macro[]>("get_macros"));
    } catch {
      setMacros([]);
    }
  }

  useEffect(() => {
    load();
  }, []);

  async function add() {
    if (!trigger.trim() || !expansion.trim()) return;
    await invoke("add_macro", {
      trigger: trigger.trim(),
      expansion: expansion.trim(),
    });
    setTrigger("");
    setExpansion("");
    await load();
  }

  async function remove(id: number) {
    await invoke("delete_macro", { id });
    await load();
  }

  return (
    <div className="page-shell space-y-5">
      <header>
        <h1 className="page-title">Snippets</h1>
        <p className="page-subtitle">
          Say a trigger phrase — MaxSpeech expands it to canned text.
        </p>
      </header>

      <div className="surface-card p-4 space-y-3">
        <input
          value={trigger}
          onChange={(e) => setTrigger(e.target.value)}
          placeholder='Trigger (e.g. "sign off")'
          className="input-field w-full px-3 py-2.5 text-sm"
        />
        <textarea
          value={expansion}
          onChange={(e) => setExpansion(e.target.value)}
          placeholder="Expansion text…"
          rows={3}
          className="input-field w-full px-3 py-2.5 text-sm resize-none"
        />
        <button
          onClick={add}
          disabled={!trigger.trim() || !expansion.trim()}
          className="btn-primary px-4 py-2 text-sm"
        >
          Add snippet
        </button>
      </div>

      <div className="space-y-2">
        {macros.length === 0 && (
          <div className="surface-card p-8 text-center text-sm text-[var(--ms-text-dim)]">
            No snippets yet.
          </div>
        )}
        {macros.map((m) => (
          <div key={m.id} className="surface-card p-4 space-y-2">
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs px-2 py-0.5 rounded-full bg-[var(--ms-turquoise-glow)] text-[var(--ms-turquoise)] truncate max-w-[70%]">
                {m.trigger}
              </span>
              <button
                onClick={() => remove(m.id)}
                className="text-xs text-[var(--ms-text-dim)] hover:text-[var(--ms-error)] shrink-0"
              >
                Remove
              </button>
            </div>
            <p className="text-sm text-[var(--ms-text-soft)] whitespace-pre-wrap break-words">{m.expansion}</p>
          </div>
        ))}
      </div>
    </div>
  );
}
