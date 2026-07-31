import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { friendlyAppName } from "../../lib/appNames";
import Toggle from "../../components/Toggle";

interface AppProfile {
  id: number;
  exe_pattern: string;
  title_pattern: string;
  tone: string;
  enabled: boolean;
}

const TONES = [
  { id: "casual", label: "Casual", hint: "Terse, chatty — Slack & Discord" },
  { id: "formal", label: "Formal", hint: "Professional — email & Outlook" },
  { id: "code", label: "Code", hint: "Technical — Cursor & VS Code" },
  { id: "prose", label: "Prose", hint: "Clean paragraphs — Word & Notion" },
  { id: "default", label: "Default", hint: "Light cleanup, keep your voice" },
] as const;

export default function StylePage() {
  const [profiles, setProfiles] = useState<AppProfile[]>([]);
  const [savingId, setSavingId] = useState<number | null>(null);

  async function load() {
    try {
      setProfiles(await invoke<AppProfile[]>("get_app_profiles"));
    } catch {
      setProfiles([]);
    }
  }

  useEffect(() => {
    load();
  }, []);

  async function updateTone(id: number, tone: string) {
    setSavingId(id);
    try {
      await invoke("update_app_profile", { id, tone, enabled: null });
      setProfiles((prev) => prev.map((p) => (p.id === id ? { ...p, tone } : p)));
    } finally {
      setSavingId(null);
    }
  }

  async function toggleEnabled(id: number, enabled: boolean) {
    setSavingId(id);
    try {
      await invoke("update_app_profile", { id, tone: null, enabled });
      setProfiles((prev) => prev.map((p) => (p.id === id ? { ...p, enabled } : p)));
    } finally {
      setSavingId(null);
    }
  }

  return (
    <div className="page-shell space-y-5">
      <header>
        <h1 className="page-title">Style</h1>
        <p className="page-subtitle">
          Pick how MaxSpeech should sound in each app.
        </p>
      </header>

      <div className="space-y-3">
        {profiles.map((p) => {
          const appLabel = p.title_pattern
            ? `${friendlyAppName(p.exe_pattern)} · ${p.title_pattern}`
            : friendlyAppName(p.exe_pattern);
          const toneMeta = TONES.find((t) => t.id === p.tone) || TONES[4];
          return (
            <div
              key={p.id}
              className={`surface-card p-4 space-y-3 ${!p.enabled ? "opacity-55" : ""}`}
            >
              <div className="flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <div className="text-sm font-medium truncate">{appLabel}</div>
                  <p className="text-xs text-[var(--ms-text-dim)] mt-1">{toneMeta.hint}</p>
                </div>
                <Toggle
                  checked={p.enabled}
                  disabled={savingId === p.id}
                  label={`Enable ${appLabel}`}
                  onChange={() => toggleEnabled(p.id, !p.enabled)}
                />
              </div>

              <div className="flex flex-wrap gap-1.5">
                {TONES.map((t) => (
                  <button
                    key={t.id}
                    disabled={savingId === p.id}
                    onClick={() => updateTone(p.id, t.id)}
                    className={`text-xs px-3 py-1.5 rounded-full transition-colors ${
                      p.tone === t.id
                        ? t.id === "formal" || t.id === "prose"
                          ? "bg-[var(--ms-orange-glow)] text-[var(--ms-orange)]"
                          : "bg-[var(--ms-turquoise-glow)] text-[var(--ms-turquoise)]"
                        : "text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)]"
                    }`}
                    style={
                      p.tone === t.id
                        ? undefined
                        : { background: "var(--ms-fill-muted)" }
                    }
                  >
                    {t.label}
                  </button>
                ))}
              </div>
            </div>
          );
        })}
        {profiles.length === 0 && (
          <div className="surface-card p-8 text-center text-sm text-[var(--ms-text-dim)]">
            Default profiles will appear after first launch.
          </div>
        )}
      </div>
    </div>
  );
}
