import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import { formatHotkey } from "../Shell";
import Toggle from "../../components/Toggle";
import {
  checkForUpdate,
  getAppVersion,
  installAvailableUpdate,
  type UpdateInfo,
} from "../../lib/updater";
import {
  PLAN_OPTIONS,
  formatWeeklyUsage,
  weeklyUsagePct,
  type PlanStatus,
  type PlanTier,
} from "../../lib/plan";
import {
  THEME_OPTIONS,
  normalizeTheme,
  persistTheme,
  type UiTheme,
} from "../../lib/theme";
import ConfirmModal from "../../components/ConfirmModal";
import type { PageId } from "../Shell";
import type { AuthUser } from "../../lib/auth";
import { signOut, updateCloudPlan } from "../../lib/auth";
import {
  pushCloudSettings,
  syncAllLocalHistoryIfMax,
} from "../../lib/cloudSync";

const PRESET_KEYS = [
  "ctrl+super", // Ctrl + Win (Fn is hardware-only on most Windows keyboards)
  "ctrl+space",
  "alt+space",
  "ctrl+shift+space",
  "ctrl+`",
  "f8",
  "f9",
  "ctrl+shift+d",
];

export default function SettingsPage({
  authUser,
  onChanged,
  onUpdateFound,
  onNavigate,
  onEnterSelectDelete,
}: {
  authUser?: AuthUser | null;
  onChanged: () => void;
  onUpdateFound?: (info: UpdateInfo | null) => void;
  onNavigate?: (p: PageId) => void;
  onEnterSelectDelete?: () => void;
}) {
  const [autostart, setAutostart] = useState(false);
  const [showLive, setShowLive] = useState(true);
  const [aiEnhance, setAiEnhance] = useState(true);
  const [soundCue, setSoundCue] = useState(false);
  const [insertSpace, setInsertSpace] = useState(true);
  const [hotkey, setHotkey] = useState("ctrl+space");
  const [hotkeyMode, setHotkeyMode] = useState<"hold" | "toggle">("hold");
  const [capturing, setCapturing] = useState(false);
  const [hotkeyMsg, setHotkeyMsg] = useState("");
  const captureRef = useRef<HTMLButtonElement>(null);
  const [appVersion, setAppVersion] = useState("…");
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [updatePct, setUpdatePct] = useState<number | null>(null);
  const [updateMsg, setUpdateMsg] = useState("");
  const [plan, setPlan] = useState<PlanStatus | null>(null);
  const [planMsg, setPlanMsg] = useState("");
  const [settingTier, setSettingTier] = useState<PlanTier | null>(null);
  const [uiTheme, setUiTheme] = useState<UiTheme>("dark");
  const [logoutOpen, setLogoutOpen] = useState(false);
  const [loggingOut, setLoggingOut] = useState(false);

  useEffect(() => {
    refresh();
    isEnabled().then(setAutostart).catch(() => {});
    getAppVersion().then(setAppVersion).catch(() => setAppVersion("0.1.0"));
  }, []);

  useEffect(() => {
    if (!capturing) return;
    const onKey = async (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setCapturing(false);
        return;
      }
      const k = e.key.toLowerCase();
      if (["control", "shift", "alt", "meta"].includes(k)) return;

      const parts: string[] = [];
      if (e.ctrlKey) parts.push("ctrl");
      if (e.altKey) parts.push("alt");
      if (e.shiftKey) parts.push("shift");
      if (e.metaKey) parts.push("super");

      let key = k;
      if (key === " ") key = "space";
      if (key.startsWith("arrow")) key = key.replace("arrow", "");
      if (
        key.length === 1 ||
        key.startsWith("f") ||
        ["space", "tab", "enter", "backspace"].includes(key)
      ) {
        parts.push(key);
      } else {
        return;
      }

      const combo = parts.join("+");
      setCapturing(false);
      try {
        await invoke("set_hotkey", { shortcut: combo });
        setHotkey(combo);
        setHotkeyMsg(`Hotkey set to ${formatHotkey(combo)}`);
        void pushCloudSettings();
        onChanged();
      } catch (err) {
        setHotkeyMsg(`Could not set hotkey: ${err}`);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [capturing, onChanged]);

  async function refresh() {
    try {
      const live = await invoke<string>("get_setting", { key: "show_live_transcript" });
      setShowLive(live !== "false");
      const enhance = await invoke<string>("get_setting", { key: "ai_enhance" });
      setAiEnhance(enhance !== "false");
      const sound = await invoke<string>("get_setting", { key: "sound_cue" });
      setSoundCue(sound === "true");
      const space = await invoke<string>("get_setting", { key: "trailing_space" });
      setInsertSpace(space !== "false");
      const theme = await invoke<string>("get_setting", { key: "ui_theme" });
      setUiTheme(normalizeTheme(theme));
      setHotkey(await invoke<string>("get_hotkey"));
      const mode = await invoke<string>("get_hotkey_mode");
      setHotkeyMode(mode === "toggle" ? "toggle" : "hold");
      try {
        setPlan(await invoke<PlanStatus>("get_plan_status"));
      } catch {
        // Backend may not be ready yet during parallel work
      }
    } catch {
      // ignore
    }
  }

  async function selectTier(tier: PlanTier) {
    if (settingTier || plan?.tier === tier) return;
    setSettingTier(tier);
    setPlanMsg("");
    try {
      await invoke("set_plan_tier", { tier });
      await updateCloudPlan(tier);
      await pushCloudSettings();
      if (tier === "max") await syncAllLocalHistoryIfMax();
      const next = await invoke<PlanStatus>("get_plan_status");
      setPlan(next);
      setPlanMsg(
        tier === "max"
          ? "Switched to Max — dictation history will sync to the cloud."
          : `Switched to ${PLAN_OPTIONS.find((p) => p.tier === tier)?.label ?? tier}.`,
      );
      onChanged();
    } catch (e) {
      setPlanMsg(`Could not set plan: ${e}`);
    } finally {
      setSettingTier(null);
    }
  }

  async function toggleAutostart() {
    if (autostart) {
      await disable();
      setAutostart(false);
    } else {
      await enable();
      setAutostart(true);
    }
  }

  async function toggleBool(
    key: string,
    current: boolean,
    setter: (v: boolean) => void,
  ) {
    const next = !current;
    setter(next);
    await invoke("set_setting", { key, value: next ? "true" : "false" });
    void pushCloudSettings();
  }

  async function applyMode(mode: "hold" | "toggle") {
    setHotkeyMode(mode);
    await invoke("set_hotkey_mode", { mode });
    void pushCloudSettings();
    onChanged();
  }

  async function applyThemeChoice(theme: UiTheme) {
    setUiTheme(theme);
    await persistTheme(theme);
    void pushCloudSettings();
  }

  async function applyPreset(combo: string) {
    try {
      await invoke("set_hotkey", { shortcut: combo });
      setHotkey(combo);
      setHotkeyMsg(`Hotkey set to ${formatHotkey(combo)}`);
      void pushCloudSettings();
      onChanged();
    } catch (err) {
      setHotkeyMsg(`Could not set hotkey: ${err}`);
    }
  }

  async function runUpdateCheck() {
    setCheckingUpdate(true);
    setUpdateMsg("");
    try {
      const info = await checkForUpdate();
      setUpdateInfo(info);
      onUpdateFound?.(info);
      setUpdateMsg(info ? `Version ${info.version} is available.` : "You're on the latest version.");
    } catch {
      setUpdateMsg("Couldn't check for updates right now.");
    } finally {
      setCheckingUpdate(false);
    }
  }

  async function applyUpdate() {
    if (updating) return;
    setUpdating(true);
    setUpdatePct(0);
    setUpdateMsg("Downloading update…");
    try {
      await installAvailableUpdate((pct) => setUpdatePct(pct));
    } catch (e) {
      setUpdateMsg(`Update failed: ${e}`);
      setUpdating(false);
      setUpdatePct(null);
    }
  }

  function enterSelectDelete() {
    onEnterSelectDelete?.();
    onNavigate?.("home");
  }

  async function confirmLogout() {
    if (loggingOut) return;
    setLoggingOut(true);
    try {
      await signOut();
      await invoke("clear_session");
    } catch (e) {
      console.error(e);
      setLoggingOut(false);
      setLogoutOpen(false);
    }
  }

  return (
    <div className="page-shell space-y-7">
      <header>
        <h1 className="page-title">Settings</h1>
        <p className="page-subtitle">
          Account and preferences sync to your MaxSpeech cloud. Home history stays on this PC unless you&apos;re on Max.
        </p>
      </header>

      {/* Account */}
      {authUser && (
        <section className="space-y-2.5">
          <h2 className="settings-section-title">Account</h2>
          <div className="settings-group space-y-0">
            <div className="settings-row">
              <div className="settings-row-text">
                <div className="settings-row-title">{authUser.username}</div>
                <div className="settings-row-desc">{authUser.email}</div>
              </div>
            </div>
            <div className="settings-row">
              <div className="settings-row-text">
                <div className="settings-row-title">User ID</div>
                <div className="settings-row-desc font-mono text-[11px] break-all">
                  {authUser.id}
                </div>
              </div>
            </div>
          </div>
        </section>
      )}

      {/* Plan & usage */}
      <section className="space-y-2.5">
        <h2 className="settings-section-title">Plan &amp; usage</h2>
        <div className="settings-group">
          <div className="settings-row">
            <div className="settings-row-text">
              <div className="settings-row-title">Words this week</div>
              <div className="settings-row-desc">
                {plan
                  ? !plan.can_dictate
                    ? plan.tier === "max"
                      ? "Weekly limit reached — resets Monday (UTC)"
                      : "Weekly limit reached — upgrade to keep dictating"
                    : "Resets every Monday (UTC)"
                  : "Loading…"}
              </div>
            </div>
            <span className="text-sm text-[var(--ms-turquoise)] font-medium shrink-0 tabular-nums">
              {plan ? formatWeeklyUsage(plan) : "—"}
            </span>
          </div>

          {plan && plan.weekly_limit != null && (
            <div className="px-4 pb-4">
              <div
                className="h-1.5 rounded-full overflow-hidden"
                style={{ background: "var(--ms-fill-track)" }}
              >
                <div
                  className={`h-full rounded-full transition-all ${
                    (weeklyUsagePct(plan) ?? 0) >= 100
                      ? "bg-[var(--ms-orange)]"
                      : "bg-[var(--ms-turquoise)]"
                  }`}
                  style={{ width: `${weeklyUsagePct(plan) ?? 0}%` }}
                />
              </div>
            </div>
          )}
        </div>

        <div className="settings-group p-3">
          <div className="grid grid-cols-2 gap-2">
            {PLAN_OPTIONS.map((opt) => {
              const active = plan?.tier === opt.tier;
              const busy = settingTier === opt.tier;
              return (
                <button
                  key={opt.tier}
                  onClick={() => selectTier(opt.tier)}
                  disabled={!!settingTier || active}
                  className={`text-left p-3 rounded-2xl transition-all disabled:cursor-default ${
                    active
                      ? "bg-[var(--ms-turquoise-glow)] ring-1 ring-[var(--ms-turquoise)]/40"
                      : "text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)]"
                  }`}
                  style={active ? undefined : { background: "var(--ms-fill-muted)" }}
                >
                  <div className="flex items-baseline justify-between gap-2">
                    <div
                      className={`text-sm font-medium ${
                        active ? "text-[var(--ms-turquoise)]" : ""
                      }`}
                    >
                      {opt.label}
                    </div>
                    <div
                      className={`text-xs font-semibold ${
                        active ? "text-[var(--ms-turquoise)]" : "text-[var(--ms-orange)]"
                      }`}
                    >
                      {opt.price}
                      {opt.tier !== "free" ? <span className="opacity-60">/mo</span> : null}
                    </div>
                  </div>
                  <div className="text-[11px] mt-1 opacity-80 leading-snug">{opt.limit}</div>
                  <div
                    className={`text-[10px] mt-2 font-medium ${
                      active ? "text-[var(--ms-turquoise)]" : "opacity-60"
                    }`}
                  >
                    {busy ? "Switching…" : active ? "Current plan" : "Select"}
                  </div>
                </button>
              );
            })}
          </div>
          <p className="text-[11px] text-[var(--ms-text-dim)] leading-relaxed mt-3 px-1">
            Payment checkout coming soon — plans are selectable for testing.
            Home dictation history stays on this device; cloud history sync is included with Max ($10).
          </p>
          {planMsg && (
            <p className="text-xs text-[var(--ms-turquoise)] mt-2 px-1">{planMsg}</p>
          )}
        </div>
      </section>

      {/* Hotkey */}
      <section className="space-y-2.5">
        <h2 className="settings-section-title">Hotkey</h2>
        <div className="settings-group">
          <div className="settings-row">
            <div className="settings-row-text">
              <div className="settings-row-title">Dictation shortcut</div>
              <div className="settings-row-desc">
                Current:{" "}
                <span className="text-[var(--ms-turquoise)] font-medium">
                  {formatHotkey(hotkey)}
                </span>
              </div>
            </div>
            <button
              ref={captureRef}
              onClick={() => {
                setHotkeyMsg("");
                setCapturing(true);
              }}
              className={`px-3.5 py-1.5 text-xs rounded-full font-semibold shrink-0 transition-all ${
                capturing
                  ? "bg-[var(--ms-orange)] text-black animate-pulse"
                  : "btn-primary"
              }`}
            >
              {capturing ? "Press keys…" : "Change"}
            </button>
          </div>

          <div className="px-4 pb-4 pt-1">
            <div className="flex flex-wrap gap-1.5">
              {PRESET_KEYS.map((p) => (
                <button
                  key={p}
                  onClick={() => applyPreset(p)}
                  className={`text-[11px] px-2.5 py-1 rounded-full transition-colors ${
                    hotkey === p
                      ? "bg-[var(--ms-turquoise-glow)] text-[var(--ms-turquoise)]"
                      : "text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)]"
                  }`}
                  style={hotkey === p ? undefined : { background: "var(--ms-fill-muted)" }}
                >
                  {formatHotkey(p)}
                </button>
              ))}
            </div>
            {hotkeyMsg && (
              <p className="text-xs text-[var(--ms-turquoise)] mt-2">{hotkeyMsg}</p>
            )}
          </div>
        </div>

        <div className="settings-group p-3">
          <div className="text-xs text-[var(--ms-text-dim)] mb-2 px-1">Activation</div>
          <div className="grid grid-cols-2 gap-2">
            <ModeBtn
              active={hotkeyMode === "hold"}
              title="Press & hold"
              desc="Hold to talk, release to finish"
              onClick={() => applyMode("hold")}
            />
            <ModeBtn
              active={hotkeyMode === "toggle"}
              title="Press only"
              desc="Tap start, tap again to stop"
              onClick={() => applyMode("toggle")}
            />
          </div>
        </div>
      </section>

      {/* Dictation toggles */}
      <section className="space-y-2.5">
        <h2 className="settings-section-title" style={{ color: "var(--ms-orange)" }}>
          Dictation
        </h2>
        <div className="settings-group">
          <SettingsToggle
            title="Show live transcript"
            desc="Show words on the bottom bar while speaking"
            checked={showLive}
            onChange={() => toggleBool("show_live_transcript", showLive, setShowLive)}
          />
          <SettingsToggle
            title="AI enhance"
            desc="Clean grammar, fillers, and self-corrections."
            checked={aiEnhance}
            onChange={() => toggleBool("ai_enhance", aiEnhance, setAiEnhance)}
          />
          <SettingsToggle
            title="Trailing space"
            desc="Add a space after each insertion so you can keep typing"
            checked={insertSpace}
            onChange={() => toggleBool("trailing_space", insertSpace, setInsertSpace)}
          />
          <SettingsToggle
            title="Start / stop sound"
            desc="Soft cue when dictation starts and stops"
            checked={soundCue}
            onChange={() => toggleBool("sound_cue", soundCue, setSoundCue)}
          />
          <div className="settings-row">
            <div className="settings-row-text">
              <div className="settings-row-title">Max recording</div>
              <div className="settings-row-desc">
                Hard cap: 2 minutes wall-clock per session
              </div>
            </div>
            <span className="text-sm text-[var(--ms-turquoise)] font-medium shrink-0">
              2 min
            </span>
          </div>
        </div>
      </section>

      {/* Theme */}
      <section className="space-y-2.5">
        <h2 className="settings-section-title">Theme</h2>
        <div className="settings-group p-3">
          <div className="grid grid-cols-3 gap-2">
            {THEME_OPTIONS.map((opt) => (
              <ModeBtn
                key={opt.id}
                active={uiTheme === opt.id}
                title={opt.label}
                desc={opt.desc}
                onClick={() => applyThemeChoice(opt.id)}
              />
            ))}
          </div>
        </div>
      </section>

      {/* General */}
      <section className="space-y-2.5">
        <h2 className="settings-section-title" style={{ color: "var(--ms-orange)" }}>
          General
        </h2>
        <div className="settings-group">
          <SettingsToggle
            title="Launch at startup"
            desc="Start MaxSpeech when Windows logs in"
            checked={autostart}
            onChange={toggleAutostart}
          />
        </div>
      </section>

      {/* About */}
      <section className="space-y-2.5 pb-4">
        <h2 className="settings-section-title">About &amp; updates</h2>
        <div className="settings-group">
          <div className="settings-row">
            <div className="settings-row-text">
              <div className="settings-row-title">MaxSpeech</div>
              <div className="settings-row-desc">
                Version{" "}
                <span className="text-[var(--ms-turquoise)]">{appVersion}</span>
              </div>
            </div>
            <button
              onClick={runUpdateCheck}
              disabled={checkingUpdate || updating}
              className="px-3.5 py-1.5 text-xs rounded-full text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)] transition-colors disabled:opacity-50 shrink-0"
              style={{ background: "var(--ms-fill-muted)" }}
            >
              {checkingUpdate ? "Checking…" : "Check"}
            </button>
          </div>

          {updateInfo && (
            <div className="mx-3 mb-3 rounded-2xl bg-[var(--ms-turquoise-glow)] p-3 space-y-3">
              <div className="text-sm font-medium text-[var(--ms-turquoise)]">
                Update available — v{updateInfo.version}
              </div>
              {updateInfo.body && (
                <p className="text-xs text-[var(--ms-text-soft)] whitespace-pre-wrap">
                  {updateInfo.body}
                </p>
              )}
              <button
                onClick={applyUpdate}
                disabled={updating}
                className="btn-primary px-4 py-2 text-xs"
              >
                {updating
                  ? updatePct != null
                    ? `Installing ${updatePct}%…`
                    : "Installing…"
                  : "Update now"}
              </button>
            </div>
          )}

          {updateMsg && (
            <p className="px-4 pb-3 text-xs text-[var(--ms-text-dim)]">{updateMsg}</p>
          )}
        </div>
      </section>

      {/* Danger zone */}
      <section className="space-y-2.5 pb-6">
        <h2 className="settings-section-title" style={{ color: "var(--ms-error)" }}>
          Danger zone
        </h2>
        <div className="danger-zone">
          <div className="p-4 space-y-3" style={{ borderBottom: "1px solid rgba(239,68,68,0.18)" }}>
            <div>
              <div className="text-sm font-medium text-[var(--ms-text)]">Delete history</div>
              <p className="text-[11px] text-[var(--ms-text-dim)] mt-1 leading-relaxed">
                Pick dictations on Home and remove them in bulk.
              </p>
            </div>
            <button
              type="button"
              onClick={enterSelectDelete}
              className="w-full px-4 py-2.5 text-xs font-semibold rounded-full transition-colors text-[var(--ms-error)] hover:bg-[var(--ms-error)]/15"
              style={{ background: "rgba(239,68,68,0.1)" }}
            >
              Select chats to delete
            </button>
          </div>
          <div className="p-4 space-y-3">
            <div>
              <div className="text-sm font-medium text-[var(--ms-text)]">Log out</div>
              <p className="text-[11px] text-[var(--ms-text-dim)] mt-1 leading-relaxed">
                Clears saved keys, history, and local session — returns you to onboarding.
              </p>
            </div>
            <button
              type="button"
              onClick={() => setLogoutOpen(true)}
              className="w-full px-4 py-2.5 text-sm font-semibold rounded-full bg-[var(--ms-error)] text-white hover:brightness-110 transition-all shadow-[0_0_18px_rgba(239,68,68,0.35)]"
            >
              Log out
            </button>
          </div>
        </div>
      </section>

      <ConfirmModal
        open={logoutOpen}
        title="Would you like to confirm to log out?"
        description="Signs you out of MaxSpeech cloud and clears local history and session preferences on this PC."
        confirmLabel="Log out"
        destructive
        busy={loggingOut}
        onCancel={() => {
          if (!loggingOut) setLogoutOpen(false);
        }}
        onConfirm={confirmLogout}
      />
    </div>
  );
}

function SettingsToggle({
  title,
  desc,
  checked,
  onChange,
}: {
  title: string;
  desc: string;
  checked: boolean;
  onChange: () => void;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-text">
        <div className="settings-row-title">{title}</div>
        <div className="settings-row-desc">{desc}</div>
      </div>
      <Toggle checked={checked} onChange={onChange} label={title} />
    </div>
  );
}

function ModeBtn({
  active,
  title,
  desc,
  onClick,
}: {
  active: boolean;
  title: string;
  desc: string;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`text-left p-3 rounded-2xl transition-all ${
        active
          ? "bg-[var(--ms-turquoise-glow)] ring-1 ring-[var(--ms-turquoise)]/40"
          : "text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)]"
      }`}
      style={active ? undefined : { background: "var(--ms-fill-muted)" }}
    >
      <div className={`text-sm font-medium ${active ? "text-[var(--ms-turquoise)]" : ""}`}>
        {title}
      </div>
      <div className="text-[11px] mt-1 opacity-80 leading-snug">{desc}</div>
    </button>
  );
}
