import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface HistoryEntry {
  id: number;
  text: string;
  app_name: string;
  created_at: string;
}

export default function History() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [search, setSearch] = useState("");

  useEffect(() => {
    loadHistory();
  }, []);

  async function loadHistory() {
    try {
      const data = await invoke<HistoryEntry[]>("get_history", { search });
      setEntries(data);
    } catch {
      // store not ready yet
    }
  }

  useEffect(() => {
    const timeout = setTimeout(loadHistory, 300);
    return () => clearTimeout(timeout);
  }, [search]);

  return (
    <div className="flex flex-col h-screen bg-[var(--ms-bg)]">
      <header className="p-4 border-b border-[var(--ms-border)]">
        <h1 className="text-lg font-semibold mb-3">Dictation History</h1>
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search history..."
          className="w-full px-3 py-2 rounded-lg bg-[var(--ms-surface)] border border-[var(--ms-border)] text-sm focus:outline-none focus:border-indigo-500"
        />
      </header>
      <main className="flex-1 overflow-y-auto p-4 space-y-2">
        {entries.length === 0 && (
          <p className="text-sm text-[var(--ms-text-dim)] text-center mt-8">
            No dictation history yet.
          </p>
        )}
        {entries.map((entry) => (
          <div
            key={entry.id}
            className="p-3 rounded-lg bg-[var(--ms-surface)] border border-[var(--ms-border)]"
          >
            <p className="text-sm">{entry.text}</p>
            <div className="flex items-center gap-2 mt-2 text-xs text-[var(--ms-text-dim)]">
              <span>{entry.app_name}</span>
              <span>&middot;</span>
              <span>{new Date(entry.created_at).toLocaleString()}</span>
            </div>
          </div>
        ))}
      </main>
    </div>
  );
}
