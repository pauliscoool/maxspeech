import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Tab = "general" | "hotkeys" | "profiles" | "dictionary" | "macros" | "api";

export default function Settings() {
  const [tab, setTab] = useState<Tab>("general");
  const [deepgramKey, setDeepgramKey] = useState("");
  const [llmKey, setLlmKey] = useState("");
  const [saving, setSaving] = useState(false);

  const tabs: { id: Tab; label: string }[] = [
    { id: "general", label: "General" },
    { id: "hotkeys", label: "Hotkeys" },
    { id: "profiles", label: "App Profiles" },
    { id: "dictionary", label: "Dictionary" },
    { id: "macros", label: "Macros" },
    { id: "api", label: "API Keys" },
  ];

  async function saveApiKeys() {
    setSaving(true);
    try {
      if (deepgramKey)
        await invoke("save_secret", { key: "deepgram_api_key", value: deepgramKey });
      if (llmKey)
        await invoke("save_secret", { key: "llm_api_key", value: llmKey });
      setDeepgramKey("");
      setLlmKey("");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex h-screen bg-[var(--ms-bg)]">
      <nav className="w-48 border-r border-[var(--ms-border)] p-4 flex flex-col gap-1">
        <h1 className="text-lg font-semibold mb-4">MaxSpeech</h1>
        {tabs.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            className={`text-left px-3 py-2 rounded-lg text-sm transition-colors ${
              tab === t.id
                ? "bg-indigo-500/20 text-indigo-400"
                : "text-[var(--ms-text-dim)] hover:text-[var(--ms-text)] hover:bg-[var(--ms-surface)]"
            }`}
          >
            {t.label}
          </button>
        ))}
      </nav>
      <main className="flex-1 p-6 overflow-y-auto">
        {tab === "general" && (
          <div>
            <h2 className="text-xl font-semibold mb-4">General</h2>
            <label className="flex items-center gap-3 text-sm">
              <input type="checkbox" defaultChecked className="accent-indigo-500" />
              Launch at startup
            </label>
          </div>
        )}
        {tab === "hotkeys" && (
          <div>
            <h2 className="text-xl font-semibold mb-4">Hotkeys</h2>
            <div className="space-y-3">
              <div className="flex items-center justify-between p-3 rounded-lg bg-[var(--ms-surface)]">
                <span className="text-sm">Push to Talk</span>
                <kbd className="px-2 py-1 rounded bg-[var(--ms-border)] text-xs">Ctrl + Space</kbd>
              </div>
              <div className="flex items-center justify-between p-3 rounded-lg bg-[var(--ms-surface)]">
                <span className="text-sm">Toggle Dictation</span>
                <kbd className="px-2 py-1 rounded bg-[var(--ms-border)] text-xs">Ctrl + Shift + Space</kbd>
              </div>
            </div>
          </div>
        )}
        {tab === "profiles" && (
          <div>
            <h2 className="text-xl font-semibold mb-4">App Profiles</h2>
            <p className="text-sm text-[var(--ms-text-dim)]">
              Tone profiles adjust dictation style based on the active application.
            </p>
          </div>
        )}
        {tab === "dictionary" && (
          <div>
            <h2 className="text-xl font-semibold mb-4">Custom Dictionary</h2>
            <p className="text-sm text-[var(--ms-text-dim)]">
              Words and phrases to improve recognition accuracy.
            </p>
          </div>
        )}
        {tab === "macros" && (
          <div>
            <h2 className="text-xl font-semibold mb-4">Snippets &amp; Macros</h2>
            <p className="text-sm text-[var(--ms-text-dim)]">
              Spoken triggers that expand to canned text.
            </p>
          </div>
        )}
        {tab === "api" && (
          <div>
            <h2 className="text-xl font-semibold mb-4">API Keys</h2>
            <div className="space-y-4 max-w-md">
              <div>
                <label className="block text-sm mb-1">Deepgram API Key</label>
                <input
                  type="password"
                  value={deepgramKey}
                  onChange={(e) => setDeepgramKey(e.target.value)}
                  placeholder="Enter key..."
                  className="w-full px-3 py-2 rounded-lg bg-[var(--ms-surface)] border border-[var(--ms-border)] text-sm focus:outline-none focus:border-indigo-500"
                />
              </div>
              <div>
                <label className="block text-sm mb-1">LLM API Key (OpenAI / etc.)</label>
                <input
                  type="password"
                  value={llmKey}
                  onChange={(e) => setLlmKey(e.target.value)}
                  placeholder="Enter key..."
                  className="w-full px-3 py-2 rounded-lg bg-[var(--ms-surface)] border border-[var(--ms-border)] text-sm focus:outline-none focus:border-indigo-500"
                />
              </div>
              <button
                onClick={saveApiKeys}
                disabled={saving || (!deepgramKey && !llmKey)}
                className="px-4 py-2 rounded-lg bg-indigo-500 text-white text-sm font-medium hover:bg-indigo-600 disabled:opacity-50 transition-colors"
              >
                {saving ? "Saving..." : "Save Keys"}
              </button>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
