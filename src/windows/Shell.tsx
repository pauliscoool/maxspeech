import { lazy, Suspense, useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
const HomePage = lazy(() => import("./pages/HomePage"));
const InsightsPage = lazy(() => import("./pages/InsightsPage"));
const DictionaryPage = lazy(() => import("./pages/DictionaryPage"));
const SnippetsPage = lazy(() => import("./pages/SnippetsPage"));
const StylePage = lazy(() => import("./pages/StylePage"));
const TransformsPage = lazy(() => import("./pages/TransformsPage"));
const ScratchpadPage = lazy(() => import("./pages/ScratchpadPage"));
const TranscriberPage = lazy(() => import("./pages/TranscriberPage"));
const SettingsPage = lazy(() => import("./pages/SettingsPage"));
import PlanModal from "../components/PlanModal";
import ThemeWipe from "../components/ThemeWipe";
import AppToast from "../components/AppToast";
import {
  checkForUpdate,
  installAvailableUpdate,
  type UpdateInfo,
} from "../lib/updater";
import { type PlanStatus } from "../lib/plan";
import type { AuthUser } from "../lib/auth";
import { pushHistoryIfMax, type HistoryPayload } from "../lib/cloudSync";
import { formatHotkey as formatHotkeyOs } from "../lib/platform";
import {
  PROFILE_CHANGED_EVENT,
  formatFullName,
  loadProfileIdentity,
  profileInitials,
  type ProfileIdentity,
} from "../lib/profileIdentity";

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
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updating, setUpdating] = useState(false);
  const [updatePct, setUpdatePct] = useState<number | null>(null);
  const [plan, setPlan] = useState<PlanStatus | null>(null);
  const [plansOpen, setPlansOpen] = useState(false);
  const [identity, setIdentity] = useState<ProfileIdentity | null>(null);

  function goToPage(next: PageId) {
    if (next !== "home") setSelectDeleteMode(false);
    setPage(next);
  }

  function openPlans() {
    setPlansOpen(true);
  }

  useEffect(() => {
    refresh();
  }, [page]);

  useEffect(() => {
    let cancelled = false;
    const loadIdentity = () => {
      void loadProfileIdentity({
        username: authUser?.username,
        email: authUser?.email,
      }).then((next) => {
        if (!cancelled) setIdentity(next);
      });
    };
    loadIdentity();
    window.addEventListener(PROFILE_CHANGED_EVENT, loadIdentity);
    return () => {
      cancelled = true;
      window.removeEventListener(PROFILE_CHANGED_EVENT, loadIdentity);
    };
  }, [authUser?.email, authUser?.username]);

  // Refresh stats / plan when the window is focused again.
  useEffect(() => {
    const onVis = () => {
      if (document.visibilityState === "visible") refresh();
    };
    document.addEventListener("visibilitychange", onVis);
    let unlisten: (() => void) | undefined;
    void getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused) refresh();
      })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {});
    return () => {
      document.removeEventListener("visibilitychange", onVis);
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const run = () => {
      checkForUpdate()
        .then((info) => {
          if (!cancelled) setUpdateInfo(info);
        })
        .catch(() => {});
    };
    const boot = window.setTimeout(run, 12_000);
    // Re-check periodically and when the window becomes visible again.
    const timer = window.setInterval(run, 6 * 60 * 60 * 1000);
    const onVis = () => {
      if (document.visibilityState === "visible") run();
    };
    document.addEventListener("visibilitychange", onVis);
    return () => {
      cancelled = true;
      window.clearTimeout(boot);
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", onVis);
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

  useEffect(() => {
    let unlistenNav: (() => void) | undefined;
    let unlistenPlans: (() => void) | undefined;
    void listen<string>("navigate-page", (ev) => {
      const next = ev.payload;
      if (
        next === "home" ||
        next === "insights" ||
        next === "dictionary" ||
        next === "snippets" ||
        next === "style" ||
        next === "transforms" ||
        next === "scratchpad" ||
        next === "transcriber" ||
        next === "settings"
      ) {
        goToPage(next);
      }
    }).then((fn) => {
      unlistenNav = fn;
    });
    void listen("open-plans", () => {
      openPlans();
    }).then((fn) => {
      unlistenPlans = fn;
    });
    return () => {
      unlistenNav?.();
      unlistenPlans?.();
    };
  }, []);

  async function refresh() {
    try {
      setPlan(await invoke<PlanStatus>("get_plan_status"));
    } catch {
      setPlan(null);
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
    <div className="flex h-screen bg-[var(--ms-bg)] text-[var(--ms-text)] overflow-hidden flex-col relative">
      <ThemeWipe />
      <AppToast />
      <div className="ms-shell-chrome">
      <PlanModal
        open={plansOpen}
        plan={plan}
        authUser={authUser}
        onClose={() => setPlansOpen(false)}
        onChanged={() => {
          void refresh();
        }}
      />
      {/* Settings already has its own About & updates card — skip shell chrome there. */}
      {updateInfo && page !== "settings" && (
        <div
          className="shrink-0 px-4 py-2.5 flex items-center justify-between gap-3"
          style={{
            background: "var(--ms-turquoise-glow)",
            borderBottom: "1px solid var(--ms-hairline)",
          }}
        >
          <div className="min-w-0">
            <div className="text-sm font-medium text-[var(--ms-turquoise)]">
              Update available — v{updateInfo.version}
            </div>
            <div className="text-xs text-[var(--ms-text-dim)] truncate">
              Downloads, installs, and restarts MaxSpeech automatically
            </div>
          </div>
          <button
            onClick={applyUpdate}
            disabled={updating}
            className="btn-primary px-3.5 py-1.5 text-xs shrink-0 disabled:opacity-70"
          >
            {updating
              ? updatePct != null && updatePct >= 100
                ? "Restarting…"
                : updatePct != null
                  ? `${updatePct}%`
                  : "Restarting…"
              : "Update now"}
          </button>
        </div>
      )}
      <div className="flex flex-1 min-h-0 overflow-hidden">
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
          <ProfileNavButton
            identity={identity}
            email={authUser?.email ?? ""}
            username={authUser?.username}
            active={page === "settings"}
            onOpen={() => goToPage("settings")}
          />
          {updateInfo && page !== "settings" && (
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
                  ? updatePct != null && updatePct >= 100
                    ? "Restarting…"
                    : updatePct != null
                      ? `Downloading… ${updatePct}%`
                      : "Restarting…"
                  : `v${updateInfo.version} — tap to install`}
              </div>
            </button>
          )}
        </div>
      </aside>

      <main className="flex-1 min-w-0 overflow-y-auto page-enter" key={page}>
        <Suspense
          fallback={
            <div className="p-8 text-sm text-[var(--ms-text-dim)]">Loading…</div>
          }
        >
        {page === "home" && (
          <HomePage
            displayName={
              identity?.firstName || authUser?.username
            }
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
        </Suspense>
      </main>
      </div>
      </div>
    </div>
  );
}

function ProfileNavButton({
  identity,
  email,
  username,
  active,
  onOpen,
}: {
  identity: ProfileIdentity | null;
  email: string;
  username?: string;
  active: boolean;
  onOpen: () => void;
}) {
  const first = identity?.firstName ?? "";
  const last = identity?.lastName ?? "";
  const fullName = formatFullName(first, last) || username || "Account";
  const mail = (identity?.email || email).trim();
  const avatar = identity?.avatarDataUrl;
  const [avatarBroken, setAvatarBroken] = useState(false);
  const showAvatar = avatar && !avatarBroken;

  return (
    <button
      type="button"
      onClick={onOpen}
      aria-current={active ? "page" : undefined}
      className={`nav-item w-full flex items-center gap-2 px-1.5 py-1.5 rounded-2xl text-left transition-all duration-200 ${
        active
          ? "nav-active"
          : "hover:bg-[var(--ms-surface)]"
      }`}
      aria-label="Open Settings"
    >
      {showAvatar ? (
        <img
          src={avatar}
          alt=""
          className="w-8 h-8 rounded-full object-cover shrink-0"
          onError={() => setAvatarBroken(true)}
        />
      ) : (
        <div
          className="w-8 h-8 rounded-full shrink-0 flex items-center justify-center text-[10px] font-semibold text-[var(--ms-turquoise)]"
          style={{ background: "var(--ms-surface-2)" }}
        >
          {profileInitials(first || fullName, last)}
        </div>
      )}
      <div className="min-w-0 flex-1">
        <div className="text-[12px] font-medium truncate leading-tight">
          {fullName}
        </div>
        {mail ? (
          <div className="text-[10px] text-[var(--ms-text-dim)] truncate leading-tight mt-0.5">
            {mail}
          </div>
        ) : null}
      </div>
    </button>
  );
}

export function formatHotkey(raw: string) {
  return formatHotkeyOs(raw);
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
