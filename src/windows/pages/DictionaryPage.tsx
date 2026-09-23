import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { pushCloudSettings } from "../../lib/cloudSync";

interface DictWord {
  id: number;
  word: string;
  boost: number;
}

export default function DictionaryPage() {
  const [words, setWords] = useState<DictWord[]>([]);
  const [input, setInput] = useState("");
  const [subs, setSubs] = useState<[string, string][]>([]);
  const [clearingSubs, setClearingSubs] = useState(false);

  async function load() {
    try {
      setWords(await invoke<DictWord[]>("get_dictionary"));
    } catch {
      setWords([]);
    }
  }

  async function loadSubs() {
    try {
      setSubs(await invoke<[string, string][]>("get_substitutions"));
    } catch {
      setSubs([]);
    }
  }

  useEffect(() => {
    load();
    loadSubs();
  }, []);

  async function removeSub(from: string) {
    await invoke("delete_substitution", { from });
    await loadSubs();
  }

  async function clearAllSubs() {
    setClearingSubs(true);
    try {
      await invoke("clear_substitutions");
      await loadSubs();
    } finally {
      setClearingSubs(false);
    }
  }

  async function add() {
    const w = input.trim();
    if (!w) return;
    await invoke("add_dict_word", { word: w });
    setInput("");
    await load();
    void pushCloudSettings();
  }

  async function remove(id: number) {
    await invoke("delete_dict_word", { id });
    await load();
    void pushCloudSettings();
  }

  return (
    <div className="page-shell space-y-5">
      <header>
        <h1 className="page-title">Dictionary</h1>
        <p className="page-subtitle">
          Names, jargon, and acronyms — boosted in speech recognition.
        </p>
      </header>

      <div className="flex gap-2">
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          placeholder="Add a word or phrase…"
          className="input-field flex-1 min-w-0 px-3 py-2.5 text-sm"
        />
        <button
          onClick={add}
          disabled={!input.trim()}
          className="btn-primary px-4 py-2.5 text-sm shrink-0"
        >
          Add
        </button>
      </div>

      <div className="space-y-2">
        {words.length === 0 && (
          <div className="surface-card p-8 text-center text-sm text-[var(--ms-text-dim)]">
            No custom words yet. Add names you use a lot.
          </div>
        )}
        {words.map((w) => (
          <div
            key={w.id}
            className="surface-card px-4 py-3 flex items-center justify-between gap-3"
          >
            <span className="text-sm truncate">{w.word}</span>
            <button
              onClick={() => remove(w.id)}
              className="text-xs text-[var(--ms-text-dim)] hover:text-[var(--ms-error)] transition-colors shrink-0"
            >
              Remove
            </button>
          </div>
        ))}
      </div>

      {subs.length > 0 && (
        <div className="space-y-2.5 pt-2">
          <div className="flex items-center justify-between">
            <h2 className="settings-section-title">Learned corrections</h2>
            <button
              onClick={clearAllSubs}
              disabled={clearingSubs}
              className="text-xs text-[var(--ms-text-dim)] hover:text-[var(--ms-error)] transition-colors disabled:opacity-50"
            >
              Clear all
            </button>
          </div>
          <p className="text-xs text-[var(--ms-text-dim)] -mt-1">
            Word swaps MaxSpeech picked up automatically from your edits and
            re-dictations. Remove any that don't belong.
          </p>
          <div className="space-y-2">
            {subs.map(([from, to]) => (
              <div
                key={from}
                className="surface-card px-4 py-3 flex items-center justify-between gap-3"
              >
                <span className="text-sm truncate">
                  <span className="text-[var(--ms-text-dim)]">{from}</span>
                  {" → "}
                  {to}
                </span>
                <button
                  onClick={() => removeSub(from)}
                  className="text-xs text-[var(--ms-text-dim)] hover:text-[var(--ms-error)] transition-colors shrink-0"
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
