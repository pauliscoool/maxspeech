import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { friendlyAppName } from "../../lib/appNames";
import Toggle from "../../components/Toggle";
import { pushCloudSettings } from "../../lib/cloudSync";

interface AppProfile {
  id: number;
  exe_pattern: string;
  title_pattern: string;
  tone: string;
  enabled: boolean;
}

const TONES = [
  { id: "casual", label: "Casual", hint: "Terse, chatty — Discord, Slack, WhatsApp" },
  { id: "formal", label: "Formal", hint: "Professional — email, CRM, LinkedIn" },
  { id: "code", label: "Code", hint: "Technical — editors, terminals, GitHub" },
  { id: "prose", label: "Prose", hint: "Clean paragraphs — Docs, Notion, Word" },
  { id: "default", label: "Default", hint: "Light cleanup, keep your voice" },
] as const;

type ToneFilter = "all" | (typeof TONES)[number]["id"];

const BROWSER_EXES = new Set(
  [
    "chrome.exe",
    "msedge.exe",
    "firefox.exe",
    "brave.exe",
    "opera.exe",
    "vivaldi.exe",
    "arc.exe",
    "chromium.exe",
    "google chrome.app",
    "microsoft edge.app",
    "firefox.app",
    "brave browser.app",
    "safari.app",
    "opera.app",
    "arc.app",
    "google-chrome",
    "chromium",
    "microsoft-edge",
    "brave-browser",
    "firefox",
    "opera",
  ].map((s) => s.toLowerCase()),
);

type DisplayGroup = {
  key: string;
  ids: number[];
  label: string;
  tone: string;
  enabled: boolean;
  titlePattern: string;
};

function isBrowserExe(exe: string) {
  return BROWSER_EXES.has(exe.toLowerCase());
}

function groupProfiles(profiles: AppProfile[]): DisplayGroup[] {
  const map = new Map<string, DisplayGroup>();
  for (const p of profiles) {
    const key =
      p.title_pattern && isBrowserExe(p.exe_pattern)
        ? `web:${p.title_pattern.toLowerCase()}`
        : `app:${p.exe_pattern.toLowerCase()}|${p.title_pattern.toLowerCase()}`;
    const existing = map.get(key);
    if (existing) {
      existing.ids.push(p.id);
      existing.enabled = existing.enabled && p.enabled;
      // Prefer the majority tone; keep first if mixed.
      continue;
    }
    const label =
      p.title_pattern && isBrowserExe(p.exe_pattern)
        ? `Browsers · ${p.title_pattern}`
        : p.title_pattern
          ? `${friendlyAppName(p.exe_pattern)} · ${p.title_pattern}`
          : friendlyAppName(p.exe_pattern);
    map.set(key, {
      key,
      ids: [p.id],
      label,
      tone: p.tone,
      enabled: p.enabled,
      titlePattern: p.title_pattern,
    });
  }
  return Array.from(map.values()).sort((a, b) => a.label.localeCompare(b.label));
}

export default function StylePage() {
  const [profiles, setProfiles] = useState<AppProfile[]>([]);
  const [savingKey, setSavingKey] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [toneFilter, setToneFilter] = useState<ToneFilter>("all");

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

  const groups = useMemo(() => groupProfiles(profiles), [profiles]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return groups.filter((g) => {
      if (toneFilter !== "all" && g.tone !== toneFilter) return false;
      if (!q) return true;
      return (
        g.label.toLowerCase().includes(q) ||
        g.titlePattern.toLowerCase().includes(q) ||
        g.tone.toLowerCase().includes(q)
      );
    });
  }, [groups, query, toneFilter]);

  async function updateTone(group: DisplayGroup, tone: string) {
    setSavingKey(group.key);
    try {
      await Promise.all(
        group.ids.map((id) =>
          invoke("update_app_profile", { id, tone, enabled: null }),
        ),
      );
      const idSet = new Set(group.ids);
      setProfiles((prev) =>
        prev.map((p) => (idSet.has(p.id) ? { ...p, tone } : p)),
      );
      void pushCloudSettings();
    } finally {
      setSavingKey(null);
    }
  }

  async function toggleEnabled(group: DisplayGroup, enabled: boolean) {
    setSavingKey(group.key);
    try {
      await Promise.all(
        group.ids.map((id) =>
          invoke("update_app_profile", { id, tone: null, enabled }),
        ),
      );
      const idSet = new Set(group.ids);
      setProfiles((prev) =>
        prev.map((p) => (idSet.has(p.id) ? { ...p, enabled } : p)),
      );
      void pushCloudSettings();
    } finally {
      setSavingKey(null);
    }
  }

  return (
    <div className="page-shell space-y-5">
      <header>
        <h1 className="page-title">Style</h1>
        <p className="page-subtitle">
          MaxSpeech picks a tone from the app you’re in — email stays formal,
          Discord stays casual. Search and tweak any preset.
        </p>
      </header>

      <div className="space-y-3">
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search apps (Gmail, Discord, Outlook…)"
          className="w-full px-3.5 py-2.5 rounded-xl bg-[var(--ms-surface)] border border-[var(--ms-border)] text-sm outline-none focus:border-[var(--ms-turquoise)]"
        />
        <div className="flex flex-wrap gap-1.5">
          <FilterChip
            active={toneFilter === "all"}
            label={`All (${groups.length})`}
            onClick={() => setToneFilter("all")}
          />
          {TONES.map((t) => (
            <FilterChip
              key={t.id}
              active={toneFilter === t.id}
              label={t.label}
              onClick={() => setToneFilter(t.id)}
            />
          ))}
        </div>
        <p className="text-xs text-[var(--ms-text-dim)]">
          Showing {filtered.length} apps · {profiles.length} match rules under
          the hood
        </p>
      </div>

      <div className="space-y-3">
        {filtered.map((g) => {
          const toneMeta = TONES.find((t) => t.id === g.tone) || TONES[4];
          const busy = savingKey === g.key;
          return (
            <div
              key={g.key}
              className={`surface-card p-4 space-y-3 ${!g.enabled ? "opacity-55" : ""}`}
            >
              <div className="flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <div className="text-sm font-medium truncate">{g.label}</div>
                  <p className="text-xs text-[var(--ms-text-dim)] mt-1">
                    {toneMeta.hint}
                    {g.ids.length > 1 ? ` · ${g.ids.length} browsers` : ""}
                  </p>
                </div>
                <Toggle
                  checked={g.enabled}
                  disabled={busy}
                  label={`Enable ${g.label}`}
                  onChange={() => toggleEnabled(g, !g.enabled)}
                />
              </div>

              <div className="flex flex-wrap gap-1.5">
                {TONES.map((t) => (
                  <button
                    key={t.id}
                    disabled={busy}
                    onClick={() => updateTone(g, t.id)}
                    className={`text-xs px-3 py-1.5 rounded-full transition-colors ${
                      g.tone === t.id
                        ? t.id === "formal" || t.id === "prose"
                          ? "bg-[var(--ms-orange-glow)] text-[var(--ms-orange)]"
                          : "bg-[var(--ms-turquoise-glow)] text-[var(--ms-turquoise)]"
                        : "text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)]"
                    }`}
                    style={
                      g.tone === t.id
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
        {profiles.length > 0 && filtered.length === 0 && (
          <div className="surface-card p-8 text-center text-sm text-[var(--ms-text-dim)]">
            No presets match that search.
          </div>
        )}
      </div>
    </div>
  );
}

function FilterChip({
  active,
  label,
  onClick,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`text-xs px-3 py-1.5 rounded-full transition-colors ${
        active
          ? "bg-[var(--ms-turquoise-glow)] text-[var(--ms-turquoise)]"
          : "text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)]"
      }`}
      style={active ? undefined : { background: "var(--ms-fill-muted)" }}
    >
      {label}
    </button>
  );
}
