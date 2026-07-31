import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import HomePage from "./pages/HomePage";
import InsightsPage from "./pages/InsightsPage";
import DictionaryPage from "./pages/DictionaryPage";
import SnippetsPage from "./pages/SnippetsPage";
import StylePage from "./pages/StylePage";
import TransformsPage from "./pages/TransformsPage";
import ScratchpadPage from "./pages/ScratchpadPage";
import TranscriberPage from "./pages/TranscriberPage";
import SettingsPage from "./pages/SettingsPage";
import {
  checkForUpdate,
  installAvailableUpdate,
  type UpdateInfo,
} from "../lib/updater";
import { formatWeeklyUsage, weeklyUsagePct, planLabel, type PlanStatus } from "../lib/plan";
import type { AuthUser } from "../lib/auth";
import { pushHistoryIfMax, type HistoryPayload } from "../lib/cloudSync";

export type PageId =
  | "home"
  | "insights"
  | "dictionary"
  | "snippets"
  | "style"
  | "transforms"
  | "scratchpad"
  | "transcriber"
  | "settings";

interface Stats {
  total_words: number;
  total_entries: number;
  days_active: number;
  avg_wpm: number;
}

const NAV: { id: PageId; label: string; icon: (active: boolean) => ReactNode }[] = [
  { id: "home", label: "Home", icon: (a) => <IconHome active={a} /> },
  { id: "insights", label: "Insights", icon: (a) => <IconChart active={a} /> },
  { id: "dictionary", label: "Dictionary", icon: (a) => <IconBook active={a} /> },
  { id: "snippets", label: "Snippets", icon: (a) => <IconSnippets active={a} /> },
  { id: "style", label: "Style", icon: (a) => <IconSpark active={a} /> },
  { id: "transforms", label: "Transforms", icon: (a) => <IconRefresh active={a} /> },
  { id: "scratchpad", label: "Scratchpad", icon: (a) => <IconPen active={a} /> },
  { id: "transcriber", label: "Transcriber", icon: (a) => <IconWave active={a} /> },
];

export default function Shell({ authUser }: { authUser: AuthUser | null }) {
  const [page, setPage] = useState<PageId>("home");
  const [selectDeleteMode, setSelectDeleteMode] = useState(false);
  const [stats, setStats] = useState<Stats>({
    total_words: 0,
    total_entries: 0,
    days_active: 0,
    avg_wpm: 0,
  });
  const [hotkeyLabel, setHotkeyLabel] = useState("Ctrl + Space");
  const [hotkeyMode, setHotkeyMode] = useState("hold");
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updating, setUpdating] = useState(false);
  const [updatePct, setUpdatePct] = useState<number | null>(null);
  const [plan, setPlan] = useState<PlanStatus | null>(null);

  function goToPage(next: PageId) {
    if (next !== "home") setSelectDeleteMode(false);
    setPage(next);
  }

  useEffect(() => {
    refresh();
  }, [page]);

  useEffect(() => {
    let cancelled = false;
    checkForUpdate()
      .then((info) => {
        if (!cancelled) setUpdateInfo(info);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  // Max plan only: push new dictations to the dedicated MaxSpeech cloud DB
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<HistoryPayload>("history-added", (ev) => {
      void pushHistoryIfMax(ev.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  async function refresh() {
    try {
      const s = await invoke<Stats>("get_stats");
      setStats(s);
      const hk = await invoke<string>("get_hotkey");
      setHotkeyLabel(formatHotkey(hk));
      const mode = await invoke<string>("get_hotkey_mode");
      setHotkeyMode(mode);
      try {
        setPlan(await invoke<PlanStatus>("get_plan_status"));
      } catch {
        setPlan(null);
      }
    } catch {
      // ignore during boot
    }
  }

  async function applyUpdate() {
    if (updating) return;
    setUpdating(true);
    setUpdatePct(0);
    try {
      await installAvailableUpdate((pct) => setUpdatePct(pct));
    } catch (e) {
      console.error(e);
      setUpdating(false);
      setUpdatePct(null);
    }
  }

  return (
    <div className="flex h-screen bg-[var(--ms-bg)] text-[var(--ms-text)] overflow-hidden">
      <aside
        className="w-[168px] shrink-0 bg-[var(--ms-bg-soft)] flex flex-col"
        style={{ borderRight: "1px solid var(--ms-hairline)" }}
      >
        <div className="px-3 pt-4 pb-3">
          <div className="flex items-center gap-2">
            <div className="ms-logo">
              <img
                src="/logo.png"
                srcSet="/logo.png 1x, /logo@2x.png 2x"
                alt="MaxSpeech"
                width={32}
                height={32}
                draggable={false}
              />
            </div>
            <div className="min-w-0">
              <div className="text-sm font-semibold tracking-tight leading-tight truncate">
                MaxSpeech
              </div>
              <div className="text-[10px] text-[var(--ms-text-dim)]">Local</div>
            </div>
          </div>
        </div>

        <nav className="flex-1 px-2 space-y-0.5 overflow-y-auto">
          {NAV.map((item) => {
            const active = page === item.id;
            return (
              <button
                key={item.id}
                onClick={() => goToPage(item.id)}
                className={`nav-item w-full flex items-center gap-2.5 px-2.5 py-2 rounded-full text-[13px] transition-all duration-200 ${
                  active
                    ? "nav-active"
                    : "text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)] hover:bg-[var(--ms-surface)]"
                }`}
              >
                <span className={`nav-icon shrink-0 ${active ? "nav-icon-active" : ""}`}>
                  {item.icon(active)}
                </span>
                <span className="truncate text-left">{item.label}</span>
              </button>
            );
          })}
        </nav>

        <div
          className="p-3 space-y-2"
          style={{ borderTop: "1px solid var(--ms-hairline)" }}
        >
          {plan?.weekly_limit != null && (
            <button
              onClick={() => goToPage("settings")}
              className="w-full text-left p-3 rounded-2xl hover:bg-[var(--ms-surface)] transition-colors"
              style={{ background: "var(--ms-fill-muted)" }}
            >
              <div className="flex items-baseline justify-between gap-2">
                <div className="text-xs font-medium" style={{ color: "var(--ms-text-soft)" }}>
                  {planLabel(plan.tier)} plan
                </div>
                <div
                  className={`text-[11px] font-medium tabular-nums ${
                    !plan.can_dictate
                      ? "text-[var(--ms-orange)]"
                      : "text-[var(--ms-turquoise)]"
                  }`}
                >
                  {formatWeeklyUsage(plan)}
                </div>
              </div>
              <div
                className="h-1 rounded-full overflow-hidden mt-2"
                style={{ background: "var(--ms-fill-track)" }}
              >
                <div
                  className={`h-full rounded-full ${
                    !plan.can_dictate
                      ? "bg-[var(--ms-orange)]"
                      : "bg-[var(--ms-turquoise)]"
                  }`}
                  style={{ width: `${weeklyUsagePct(plan) ?? 0}%` }}
                />
              </div>
              <div className="text-[10px] text-[var(--ms-text-dim)] mt-1.5">
                {!plan.can_dictate
                  ? plan.tier === "max"
                    ? "Limit reached — resets Monday"
                    : "Limit reached — upgrade"
                  : "words this week"}
              </div>
            </button>
          )}
          {updateInfo && (
            <button
              onClick={applyUpdate}
              disabled={updating}
              className="w-full text-left p-3 rounded-2xl bg-[var(--ms-turquoise-glow)] hover:brightness-110 transition-all disabled:opacity-70"
            >
              <div className="text-xs font-medium text-[var(--ms-turquoise)]">
                {updating
                  ? updatePct != null
                    ? `Updating… ${updatePct}%`
                    : "Updating…"
                  : "Update available"}
              </div>
              <div className="text-[11px] text-[var(--ms-text-dim)] mt-1">
                {updating
                  ? "Installing, then MaxSpeech will restart"
                  : `v${updateInfo.version} — tap to install`}
              </div>
            </button>
          )}
          <button
            onClick={() => goToPage("settings")}
            className={`nav-item w-full flex items-center gap-3 px-3 py-2.5 rounded-full text-sm transition-all ${
              page === "settings"
                ? "nav-active"
                : "text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)] hover:bg-[var(--ms-surface)]"
            }`}
          >
            <span className={`nav-icon shrink-0 ${page === "settings" ? "nav-icon-active" : ""}`}>
              <IconGear active={page === "settings"} />
            </span>
            Settings
          </button>
        </div>
      </aside>

      <main className="flex-1 min-w-0 overflow-y-auto page-enter" key={page}>
        {page === "home" && (
          <HomePage
            displayName={authUser?.username}
            onNavigate={goToPage}
            onChanged={refresh}
            selectMode={selectDeleteMode}
            onSelectModeChange={setSelectDeleteMode}
          />
        )}
        {page === "insights" && <InsightsPage />}
        {page === "dictionary" && <DictionaryPage />}
        {page === "snippets" && <SnippetsPage />}
        {page === "style" && <StylePage />}
        {page === "transforms" && <TransformsPage />}
        {page === "scratchpad" && <ScratchpadPage />}
        {page === "transcriber" && <TranscriberPage />}
        {page === "settings" && (
          <SettingsPage
            authUser={authUser}
            onChanged={refresh}
            onUpdateFound={setUpdateInfo}
            onNavigate={goToPage}
            onEnterSelectDelete={() => setSelectDeleteMode(true)}
          />
        )}
      </main>

      {(page === "home" || page === "insights") && (
        <aside
          className="w-[200px] shrink-0 bg-[var(--ms-bg-soft)] p-3 space-y-3 overflow-y-auto page-enter"
          style={{ borderLeft: "1px solid var(--ms-hairline)" }}
        >
          <div className="surface-card p-5 space-y-5">
            <StatBlock
              value={formatCount(stats.total_words)}
              label="total words"
              accent="turquoise"
            />
            <StatBlock
              value={stats.avg_wpm > 0 ? String(stats.avg_wpm) : "—"}
              label="wpm"
              accent="orange"
            />
            <StatBlock
              value={String(stats.days_active)}
              label="day streak"
              accent="turquoise"
            />
          </div>

          {plan?.weekly_limit != null && (
            <div className="surface-card p-4 space-y-3">
              <div className="flex items-baseline justify-between gap-2">
                <div className="text-sm font-medium">Weekly words</div>
                <div
                  className={`text-xs font-medium tabular-nums ${
                    !plan.can_dictate
                      ? "text-[var(--ms-orange)]"
                      : "text-[var(--ms-turquoise)]"
                  }`}
                >
                  {formatWeeklyUsage(plan)}
                </div>
              </div>
              <div
                className="h-1.5 rounded-full overflow-hidden"
                style={{ background: "var(--ms-fill-track)" }}
              >
                <div
                  className={`h-full rounded-full ${
                    !plan.can_dictate
                      ? "bg-[var(--ms-orange)]"
                      : "bg-[var(--ms-turquoise)]"
                  }`}
                  style={{ width: `${weeklyUsagePct(plan) ?? 0}%` }}
                />
              </div>
              <p className="text-xs text-[var(--ms-text-dim)] leading-relaxed">
                {!plan.can_dictate
                  ? plan.tier === "max"
                    ? "You've hit your weekly limit. It resets every Monday (UTC)."
                    : "You've hit your weekly limit. Upgrade in Settings to keep dictating."
                  : `${planLabel(plan.tier)} includes ${plan.weekly_limit.toLocaleString()} words per week.`}
              </p>
              <button
                onClick={() => goToPage("settings")}
                className="btn-primary w-full px-3 py-2 text-xs"
              >
                View plans
              </button>
            </div>
          )}

          {updateInfo && (
            <div className="surface-card p-4 space-y-3">
              <div className="text-sm font-medium text-[var(--ms-turquoise)]">
                Update ready
              </div>
              <p className="text-xs text-[var(--ms-text-dim)] leading-relaxed">
                Version {updateInfo.version} is available
                {updateInfo.body ? ` — ${updateInfo.body.slice(0, 80)}` : ""}.
              </p>
              <button
                onClick={applyUpdate}
                disabled={updating}
                className="btn-primary w-full px-3 py-2 text-xs"
              >
                {updating
                  ? updatePct != null
                    ? `Installing ${updatePct}%`
                    : "Installing…"
                  : "Update now"}
              </button>
            </div>
          )}

          <div className="surface-card p-4 space-y-3">
            <div className="text-sm font-medium">
              {hotkeyMode === "toggle" ? "Toggle to dictate" : "Hold to dictate"}
            </div>
            <p className="text-xs text-[var(--ms-text-dim)] leading-relaxed">
              {hotkeyMode === "toggle" ? "Press" : "Hold"}{" "}
              <kbd
                className="px-1.5 py-0.5 rounded-md text-[var(--ms-turquoise)] text-[10px]"
                style={{ background: "var(--ms-kbd-bg)" }}
              >
                {hotkeyLabel}
              </kbd>{" "}
              anywhere. The bar appears at the bottom of your screen.
            </p>
          </div>

        </aside>
      )}
    </div>
  );
}

function StatBlock({
  value,
  label,
  accent,
}: {
  value: string;
  label: string;
  accent: "turquoise" | "orange";
}) {
  return (
    <div>
      <div
        className={`text-3xl font-semibold tracking-tight ${
          accent === "orange" ? "text-[var(--ms-orange)]" : "text-[var(--ms-turquoise)]"
        }`}
      >
        {value}
      </div>
      <div className="text-xs text-[var(--ms-text-dim)] mt-1">{label}</div>
    </div>
  );
}

function formatCount(n: number) {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

export function formatHotkey(raw: string) {
  return raw
    .split("+")
    .map((p) => {
      const t = p.trim().toLowerCase();
      if (t === "ctrl" || t === "control") return "Ctrl";
      if (t === "alt") return "Alt";
      if (t === "shift") return "Shift";
      if (t === "super" || t === "meta" || t === "cmd" || t === "win") return "Win";
      if (t === "space") return "Space";
      return t.length === 1 ? t.toUpperCase() : t.charAt(0).toUpperCase() + t.slice(1);
    })
    .join(" + ");
}

function iconProps() {
  return {
    width: 20,
    height: 20,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 2,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
  };
}

function IconHome({ active: _a }: { active: boolean }) {
  return (
    <svg {...iconProps()}>
      <path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
      <polyline points="9 22 9 12 15 12 15 22" />
    </svg>
  );
}

function IconChart({ active: _a }: { active: boolean }) {
  return (
    <svg {...iconProps()}>
      <line x1="18" x2="18" y1="20" y2="10" />
      <line x1="12" x2="12" y1="20" y2="4" />
      <line x1="6" x2="6" y1="20" y2="14" />
    </svg>
  );
}

function IconBook({ active: _a }: { active: boolean }) {
  return (
    <svg {...iconProps()}>
      <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
      <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
    </svg>
  );
}

function IconSnippets({ active: _a }: { active: boolean }) {
  return (
    <svg {...iconProps()}>
      <path d="M8 6h13" />
      <path d="M8 12h13" />
      <path d="M8 18h13" />
      <path d="M3 6h.01" />
      <path d="M3 12h.01" />
      <path d="M3 18h.01" />
    </svg>
  );
}

function IconSpark({ active: _a }: { active: boolean }) {
  return (
    <svg {...iconProps()}>
      <path d="m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3z" />
    </svg>
  );
}

function IconRefresh({ active: _a }: { active: boolean }) {
  return (
    <svg {...iconProps()}>
      <path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" />
      <path d="M21 3v5h-5" />
      <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" />
      <path d="M8 16H3v5" />
    </svg>
  );
}

function IconPen({ active: _a }: { active: boolean }) {
  return (
    <svg {...iconProps()}>
      <path d="M15.5 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V8.5L15.5 3z" />
      <polyline points="14 3 14 8 19 8" />
      <line x1="8" x2="16" y1="13" y2="13" />
      <line x1="8" x2="12" y1="17" y2="17" />
    </svg>
  );
}

function IconWave({ active: _a }: { active: boolean }) {
  return (
    <svg {...iconProps()}>
      <path d="M2 10v4" />
      <path d="M6 6v12" />
      <path d="M10 3v18" />
      <path d="M14 8v8" />
      <path d="M18 5v14" />
      <path d="M22 10v4" />
    </svg>
  );
}

function IconGear({ active: _a }: { active: boolean }) {
  return (
    <svg {...iconProps()}>
      <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
      <circle cx="12" cy="12" r="3" />
    </svg>
  );
}
