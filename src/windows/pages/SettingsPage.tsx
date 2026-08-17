import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import Toggle from "../../components/Toggle";
import {
  checkForUpdate,
  getAppVersion,
  installAvailableUpdate,
  type UpdateInfo,
} from "../../lib/updater";
import {
  tierSupportsMultilingual,
  type PlanStatus,
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
import { signOut } from "../../lib/auth";
import { scheduleCloudSettingsPush, flushScheduledCloudSettingsPush } from "../../lib/cloudSync";
import {
  loadProfileIdentity,
  scheduleProfileAvatarSave,
  saveProfileNames,
  cancelScheduledAvatarSave,
  profileInitials,
  formatFullName,
  PROFILE_CHANGED_EVENT,
  PROFILE_AVATAR_SAVED_EVENT,
} from "../../lib/profileIdentity";
import {
  DEFAULT_STT_LANGUAGES,
  MAX_STT_LANGUAGES,
  STT_LANGUAGES,
  monolingualLanguages,
  parseSttLanguages,
} from "../../lib/sttLanguages";
import { openUrl } from "@tauri-apps/plugin-opener";
import { defaultHotkey, detectOs, formatHotkey } from "../../lib/platform";

const ALL_PRESET_KEYS = [
  "ctrl+super", // Ctrl + Win — Windows LL hook only
  "ctrl+space",
  "alt+space",
  "ctrl+shift+space",
  "ctrl+shift+z",
  "ctrl+`",
  "f8",
  "f9",
  "ctrl+shift+d",
];

const PRESET_KEYS =
  detectOs() === "windows"
    ? ALL_PRESET_KEYS
    : ALL_PRESET_KEYS.filter((k) => k !== "ctrl+super");

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
  const [openOnLaunch, setOpenOnLaunch] = useState(false);
  const [showLive, setShowLive] = useState(true);
  const [aiEnhance, setAiEnhance] = useState(true);
  const [soundCue, setSoundCue] = useState(false);
  const [soundCueVolume, setSoundCueVolume] = useState<"soft" | "medium" | "loud">(
    "medium",
  );
  const [insertSpace, setInsertSpace] = useState(true);
  const [hotkey, setHotkey] = useState(defaultHotkey());
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
  const [uiTheme, setUiTheme] = useState<UiTheme>("dark");
  const [logoutOpen, setLogoutOpen] = useState(false);
  const [loggingOut, setLoggingOut] = useState(false);
  const [sttMultilingual, setSttMultilingual] = useState(false);
  const [sttLanguages, setSttLanguages] = useState<string[]>([...DEFAULT_STT_LANGUAGES]);
  const [micDevices, setMicDevices] = useState<{ name: string; is_default: boolean }[]>([]);
  const [micDevice, setMicDevice] = useState("default");
  const [micMsg, setMicMsg] = useState("");
  const [micTesting, setMicTesting] = useState(false);
  const [micLevel, setMicLevel] = useState(0);
  const [firstName, setFirstName] = useState("");
  const [lastName, setLastName] = useState("");
  const [avatarUrl, setAvatarUrl] = useState<string | null>(null);
  const [profileMsg, setProfileMsg] = useState("");
  const [profileErr, setProfileErr] = useState("");
  const [savingNames, setSavingNames] = useState(false);
  const avatarInputRef = useRef<HTMLInputElement>(null);
  const profileLoadedRef = useRef(false);
  const savedNamesRef = useRef({ first: "", last: "" });

  useEffect(() => {
    refresh();
    // Mic list must not depend on the rest of settings loading — a failed
    // get_setting earlier used to skip microphones entirely.
    void loadMicrophones();
    isEnabled().then(setAutostart).catch(() => {});
    getAppVersion().then(setAppVersion).catch(() => setAppVersion("0.1.0"));
  }, []);

  useEffect(() => {
    let cancelled = false;
    const load = () => {
      void loadProfileIdentity({
        username: authUser?.username,
        email: authUser?.email,
      }).then((id) => {
        if (cancelled) return;
        setFirstName(id.firstName);
        setLastName(id.lastName);
        setAvatarUrl(id.avatarDataUrl);
        savedNamesRef.current = { first: id.firstName, last: id.lastName };
        profileLoadedRef.current = true;
      });
    };
    load();
    window.addEventListener(PROFILE_CHANGED_EVENT, load);
    return () => {
      cancelled = true;
      window.removeEventListener(PROFILE_CHANGED_EVENT, load);
    };
  }, [authUser?.email, authUser?.username]);

  useEffect(() => {
    const onSaved = () => setProfileMsg("Photo saved.");
    window.addEventListener(PROFILE_AVATAR_SAVED_EVENT, onSaved);
    return () => window.removeEventListener(PROFILE_AVATAR_SAVED_EVENT, onSaved);
  }, []);

  useEffect(() => {
    if (!micTesting) {
      setMicLevel(0);
      return;
    }
    let unlisten: (() => void) | undefined;
    void listen<number>("mic-test-level", (e) => {
      setMicLevel(typeof e.payload === "number" ? e.payload : 0);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [micTesting]);

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
      const parts: string[] = [];
      if (e.ctrlKey) parts.push("ctrl");
      if (e.altKey) parts.push("alt");
      if (e.shiftKey) parts.push("shift");
      if (e.metaKey) parts.push("super");

      // Modifier-only (e.g. Ctrl+Win): accept when 2+ modifiers are held.
      // Ctrl+Alt alone is intentionally blocked.
      if (["control", "shift", "alt", "meta"].includes(k)) {
        if (parts.length < 2) return;
        const combo = parts.join("+");
        if (combo === "ctrl+alt" || combo === "alt+ctrl") return;
        setCapturing(false);
        try {
          await invoke("set_hotkey", { shortcut: combo });
          setHotkey(combo);
          setHotkeyMsg(`Hotkey set to ${formatHotkey(combo)}`);
          scheduleCloudSettingsPush();
          onChanged();
        } catch (err) {
          setHotkeyMsg(`Could not set hotkey: ${err}`);
        }
        return;
      }

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
        scheduleCloudSettingsPush();
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
      const openLaunch = await invoke<string>("get_setting", {
        key: "open_window_on_launch",
      });
      // Default off: stay in tray at PC login unless the user opts in.
      setOpenOnLaunch(openLaunch === "true");
      const live = await invoke<string>("get_setting", { key: "show_live_transcript" });
      setShowLive(live !== "false");
      const enhance = await invoke<string>("get_setting", { key: "ai_enhance" });
      setAiEnhance(enhance !== "false");
      const sound = await invoke<string>("get_setting", { key: "sound_cue" });
      setSoundCue(sound === "true");
      const soundVol = await invoke<string>("get_setting", { key: "sound_cue_volume" });
      const volNorm = (soundVol || "medium").toLowerCase();
      setSoundCueVolume(
        volNorm === "soft" || volNorm === "loud" ? volNorm : "medium",
      );
      const space = await invoke<string>("get_setting", { key: "trailing_space" });
      setInsertSpace(space !== "false");
      const theme = await invoke<string>("get_setting", { key: "ui_theme" });
      setUiTheme(normalizeTheme(theme));
      setHotkey(await invoke<string>("get_hotkey"));
      const mode = await invoke<string>("get_hotkey_mode");
      setHotkeyMode(mode === "toggle" ? "toggle" : "hold");
      let planStatus: PlanStatus | null = null;
      try {
        planStatus = await invoke<PlanStatus>("get_plan_status");
        setPlan(planStatus);
      } catch {
        // Backend may not be ready yet during parallel work
      }
      const multiAllowed = tierSupportsMultilingual(planStatus?.tier);
      const multi = await invoke<string>("get_setting", { key: "stt_multilingual" });
      let langs = parseSttLanguages(
        await invoke<string>("get_setting", { key: "stt_languages" }),
      );
      // Free tier: force single-language mode even if old settings said otherwise.
      // Prefer English when collapsing a stale multi-select (avoids language=ru on English speech).
      if (!multiAllowed) {
        if (multi === "true") {
          await invoke("set_setting", { key: "stt_multilingual", value: "false" });
        }
        const healed = monolingualLanguages(langs);
        if (multi === "true" || healed.join() !== langs.join()) {
          langs = healed;
          await invoke("set_setting", {
            key: "stt_languages",
            value: JSON.stringify(langs),
          });
        }
        setSttMultilingual(false);
      } else if (multi !== "true" && langs.length > 1) {
        langs = monolingualLanguages(langs);
        await invoke("set_setting", {
          key: "stt_languages",
          value: JSON.stringify(langs),
        });
        setSttMultilingual(false);
      } else {
        setSttMultilingual(multi === "true");
      }
      setSttLanguages(langs);
    } catch {
      // ignore — mic list loads on its own
    }
  }

  function micNamesMatch(a: string, b: string) {
    const norm = (s: string) =>
      s
        .toLowerCase()
        .replace(/[®™©]/g, " ")
        .replace(/\s+/g, " ")
        .trim();
    return a === b || norm(a) === norm(b);
  }

  async function loadMicrophones() {
    try {
      const devices = await invoke<{ name: string; is_default: boolean }[]>(
        "list_microphones",
      );
      setMicDevices(devices);
      const saved = await invoke<string>("get_microphone");
      if (!saved || saved === "default") {
        setMicDevice("default");
        return devices;
      }
      const match = devices.find((d) => micNamesMatch(d.name, saved));
      if (match) {
        setMicDevice(match.name);
        if (match.name !== saved) {
          // Heal stored name to the live device string (trademark glyphs, etc.).
          await invoke("set_microphone", { device: match.name });
        }
      } else {
        // Keep preference selected in UI if possible; don't silently wipe it.
        setMicDevice(saved);
        setMicMsg(
          `Saved mic “${saved}” isn’t connected right now — using system default until it’s back.`,
        );
      }
      return devices;
    } catch (e) {
      setMicDevices([]);
      setMicDevice("default");
      setMicMsg(`Could not list microphones: ${e}`);
      return [];
    }
  }

  async function applyMicrophone(device: string) {
    const next = device || "default";
    setMicDevice(next);
    setMicMsg("");
    try {
      const saved = await invoke<string>("set_microphone", { device: next });
      setMicDevice(saved || next);
      scheduleCloudSettingsPush();
      const label =
        (saved || next) === "default"
          ? "System default"
          : micDevices.find((d) => micNamesMatch(d.name, saved || next))?.name ||
            saved ||
            next;
      setMicMsg(`Saved — using ${label}`);
    } catch (e) {
      setMicMsg(`Could not set microphone: ${e}`);
      try {
        setMicDevice(await invoke<string>("get_microphone"));
      } catch {
        setMicDevice("default");
      }
    }
  }

  async function testSelectedMic() {
    setMicTesting(true);
    setMicMsg("Listening… speak now");
    setMicLevel(0);
    try {
      await loadMicrophones();
      const result = await invoke<{
        name: string;
        peak: number;
        ok: boolean;
        message: string;
      }>("test_microphone");
      setMicMsg(result.message || (result.ok ? `Hearing you on ${result.name}` : "No signal"));
    } catch (e) {
      setMicMsg(`Mic test failed: ${e}`);
    } finally {
      setMicTesting(false);
    }
  }

  async function toggleAutostart() {
    if (autostart) {
      await disable();
      setAutostart(false);
      await invoke("set_setting", { key: "launch_at_startup", value: "false" });
      scheduleCloudSettingsPush();
    } else {
      await enable();
      setAutostart(true);
      await invoke("set_setting", { key: "launch_at_startup", value: "true" });
      scheduleCloudSettingsPush();
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
    scheduleCloudSettingsPush();
  }

  async function toggleMultilingual() {
    if (!tierSupportsMultilingual(plan?.tier)) return;
    const next = !sttMultilingual;
    setSttMultilingual(next);
    try {
      await invoke("set_setting", {
        key: "stt_multilingual",
        value: next ? "true" : "false",
      });
    } catch (e) {
      setSttMultilingual(false);
      console.error(e);
      return;
    }
    // Single-language mode: prefer English if it was in the multi list.
    if (!next && sttLanguages.length > 1) {
      const one = monolingualLanguages(sttLanguages);
      setSttLanguages(one);
      await invoke("set_setting", {
        key: "stt_languages",
        value: JSON.stringify(one),
      });
    }
    scheduleCloudSettingsPush();
  }

  async function toggleSttLanguage(code: string) {
    const multiOn = sttMultilingual && tierSupportsMultilingual(plan?.tier);
    let next: string[];
    if (sttLanguages.includes(code)) {
      next = sttLanguages.filter((c) => c !== code);
      if (next.length === 0) next = ["en"];
    } else if (!multiOn) {
      next = [code];
    } else if (sttLanguages.length >= MAX_STT_LANGUAGES) {
      return;
    } else {
      next = [...sttLanguages, code];
    }
    setSttLanguages(next);
    await invoke("set_setting", {
      key: "stt_languages",
      value: JSON.stringify(next),
    });
    scheduleCloudSettingsPush();
  }

  async function applyMode(mode: "hold" | "toggle") {
    setHotkeyMode(mode);
    await invoke("set_hotkey_mode", { mode });
    scheduleCloudSettingsPush();
    onChanged();
  }

  async function applyThemeChoice(theme: UiTheme) {
    setUiTheme(theme);
    await persistTheme(theme);
    scheduleCloudSettingsPush();
  }

  async function applyPreset(combo: string) {
    try {
      await invoke("set_hotkey", { shortcut: combo });
      setHotkey(combo);
      setHotkeyMsg(`Hotkey set to ${formatHotkey(combo)}`);
      scheduleCloudSettingsPush();
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
    setUpdateMsg("Checking for the latest version…");
    try {
      await installAvailableUpdate((pct) => {
        setUpdatePct(pct);
        setUpdateMsg(
          pct != null && pct >= 100
            ? "Restarting…"
            : pct != null
              ? `Downloading update… ${pct}%`
              : "Downloading update…",
        );
      });
      setUpdateMsg("Restarting…");
    } catch (e) {
      const msg = String(e).replace(/^Error:\s*/i, "");
      if (msg.includes("latest version")) {
        setUpdateInfo(null);
        onUpdateFound?.(null);
      }
      setUpdateMsg(
        msg.includes("Opened the download") || msg.includes("latest version")
          ? msg
          : `Update failed: ${msg}`,
      );
      setUpdating(false);
      setUpdatePct(null);
    }
  }

  function enterSelectDelete() {
    onEnterSelectDelete?.();
    onNavigate?.("home");
  }

  async function persistNames() {
    if (!profileLoadedRef.current || savingNames) return;
    const first = firstName.trim();
    const last = lastName.trim();
    if (
      first === savedNamesRef.current.first &&
      last === savedNamesRef.current.last
    ) {
      return;
    }
    setSavingNames(true);
    setProfileErr("");
    try {
      await saveProfileNames(first, last);
      savedNamesRef.current = { first, last };
      setProfileMsg("Name saved.");
      onChanged();
    } catch (e) {
      setProfileErr(e instanceof Error ? e.message : String(e));
    } finally {
      setSavingNames(false);
    }
  }

  async function onAvatarPicked(file: File | undefined) {
    if (!file) return;
    setProfileErr("");
    setProfileMsg("");
    try {
      const url = await scheduleProfileAvatarSave(file);
      setAvatarUrl(url);
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") return;
      setProfileErr(e instanceof Error ? e.message : String(e));
    } finally {
      if (avatarInputRef.current) avatarInputRef.current.value = "";
    }
  }

  async function confirmLogout() {
    if (loggingOut) return;
    setLoggingOut(true);
    try {
      cancelScheduledAvatarSave();
      await flushScheduledCloudSettingsPush({ toast: false });
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
              <div className="flex items-center gap-3 min-w-0 flex-1">
                {avatarUrl ? (
                  <img
                    src={avatarUrl}
                    alt=""
                    className="w-12 h-12 rounded-full object-cover shrink-0"
                  />
                ) : (
                  <div
                    className="w-12 h-12 rounded-full shrink-0 flex items-center justify-center text-sm font-semibold text-[var(--ms-turquoise)]"
                    style={{ background: "var(--ms-surface-2)" }}
                  >
                    {profileInitials(firstName, lastName) ||
                      profileInitials(authUser.username, "")}
                  </div>
                )}
                <div className="settings-row-text">
                  <div className="settings-row-title">
                    {formatFullName(firstName, lastName) || authUser.username}
                  </div>
                  <div className="settings-row-desc">
                    {authUser.email}
                    {authUser.local ? " · local only" : ""}
                  </div>
                </div>
              </div>
              <div className="shrink-0">
                <input
                  ref={avatarInputRef}
                  type="file"
                  accept="image/jpeg,image/png,image/webp,image/gif,.jpg,.jpeg,.png,.webp,.gif"
                  className="hidden"
                  onChange={(e) => void onAvatarPicked(e.target.files?.[0])}
                />
                <button
                  type="button"
                  onClick={() => avatarInputRef.current?.click()}
                  className="btn-primary px-3.5 py-1.5 text-xs"
                >
                  {avatarUrl ? "Change photo" : "Upload photo"}
                </button>
              </div>
            </div>
            <div className="settings-row settings-row-stack gap-3">
              <div className="grid grid-cols-2 gap-3 w-full">
                <label className="block space-y-1.5 min-w-0">
                  <span className="text-xs text-[var(--ms-text-dim)]">First name</span>
                  <input
                    type="text"
                    autoComplete="given-name"
                    value={firstName}
                    maxLength={64}
                    onChange={(e) => {
                      setFirstName(e.target.value);
                      setProfileMsg("");
                    }}
                    onBlur={() => void persistNames()}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.currentTarget.blur();
                      }
                    }}
                    placeholder="First"
                    className="w-full px-3 py-2 rounded-xl bg-[var(--ms-bg)] text-sm outline-none focus:shadow-[0_0_0_2px_var(--ms-turquoise-glow)]"
                  />
                </label>
                <label className="block space-y-1.5 min-w-0">
                  <span className="text-xs text-[var(--ms-text-dim)]">Last name</span>
                  <input
                    type="text"
                    autoComplete="family-name"
                    value={lastName}
                    maxLength={64}
                    onChange={(e) => {
                      setLastName(e.target.value);
                      setProfileMsg("");
                    }}
                    onBlur={() => void persistNames()}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.currentTarget.blur();
                      }
                    }}
                    placeholder="Last"
                    className="w-full px-3 py-2 rounded-xl bg-[var(--ms-bg)] text-sm outline-none focus:shadow-[0_0_0_2px_var(--ms-turquoise-glow)]"
                  />
                </label>
              </div>
              <p className="text-[11px] text-[var(--ms-text-dim)] leading-relaxed">
                JPEG, PNG, WebP, or GIF — 1 MB max. Photo and name stay on this device
                {authUser.local ? "" : " and sync to your account"}.
              </p>
              {profileErr ? (
                <p className="text-xs text-[var(--ms-error)]">{profileErr}</p>
              ) : profileMsg ? (
                <p className="text-xs text-[var(--ms-turquoise)]">{profileMsg}</p>
              ) : null}
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

      {/* Microphone */}
      <section className="space-y-2.5">
        <h2 className="settings-section-title" style={{ color: "var(--ms-orange)" }}>
          Microphone
        </h2>
        <div className="settings-group">
          <div className="settings-row settings-row-stack">
            <div className="settings-row-text">
              <div className="settings-row-title">Input device</div>
              <div className="settings-row-desc">
                Defaults to your system microphone. Tap a device to save it — used for every
                dictation.
              </div>
            </div>
            <div className="mic-device-list" role="listbox" aria-label="Microphone input device">
              <button
                type="button"
                role="option"
                aria-selected={micDevice === "default"}
                className={`mic-device-option${micDevice === "default" ? " is-active" : ""}`}
                onClick={() => void applyMicrophone("default")}
              >
                <span className="mic-device-name">System default</span>
                {micDevices.find((d) => d.is_default) ? (
                  <span className="mic-device-meta">
                    {micDevices.find((d) => d.is_default)!.name}
                  </span>
                ) : null}
              </button>
              {micDevices.map((d) => {
                const active =
                  micDevice !== "default" && micNamesMatch(micDevice, d.name);
                return (
                  <button
                    key={d.name}
                    type="button"
                    role="option"
                    aria-selected={active}
                    className={`mic-device-option${active ? " is-active" : ""}`}
                    onClick={() => void applyMicrophone(d.name)}
                  >
                    <span className="mic-device-name">{d.name}</span>
                    {d.is_default ? (
                      <span className="mic-device-meta">System default</span>
                    ) : null}
                  </button>
                );
              })}
              {micDevices.length === 0 ? (
                <p className="text-xs text-[var(--ms-text-dim)] px-1 py-1">
                  No microphones found yet. Click Refresh or Test mic.
                </p>
              ) : null}
            </div>
            <div className="flex flex-wrap gap-2 mt-1 items-center">
              <button
                type="button"
                onClick={() =>
                  void loadMicrophones().then(() => setMicMsg("Microphone list refreshed"))
                }
                className="text-[11px] px-2.5 py-1.5 rounded-full text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)] transition-colors"
                style={{ background: "var(--ms-fill-muted)" }}
              >
                Refresh list
              </button>
              <button
                type="button"
                onClick={() => void testSelectedMic()}
                disabled={micTesting}
                className="btn-primary px-3 py-1.5 text-[11px] disabled:opacity-50"
              >
                {micTesting ? "Listening…" : "Test mic"}
              </button>
              {(micTesting || micLevel > 0) && (
                <div
                  className="mic-level-track"
                  title="Live input level"
                  aria-hidden
                >
                  <div
                    className="mic-level-fill"
                    style={{ width: `${Math.round(micLevel * 100)}%` }}
                  />
                </div>
              )}
            </div>
            {micMsg ? (
              <p
                className={`text-xs mt-1 ${
                  micMsg.toLowerCase().includes("fail") ||
                  micMsg.toLowerCase().includes("could not") ||
                  micMsg.toLowerCase().includes("silence")
                    ? "text-[var(--ms-error)]"
                    : "text-[var(--ms-turquoise)]"
                }`}
              >
                {micMsg}
              </p>
            ) : null}
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
          <div className="settings-row">
            <div className="settings-row-text">
              <div className="settings-row-title">Start / stop sound</div>
              <div className="settings-row-desc">
                Custom liquid-glass chime when dictation starts and stops
              </div>
            </div>
            <div className="flex items-center gap-2 shrink-0">
              {soundCue ? (
                <div className="sound-vol-seg" role="group" aria-label="Sound cue volume">
                  {(
                    [
                      ["soft", "Soft"],
                      ["medium", "Med"],
                      ["loud", "Loud"],
                    ] as const
                  ).map(([value, label]) => (
                    <button
                      key={value}
                      type="button"
                      className={`sound-vol-opt${
                        soundCueVolume === value ? " is-active" : ""
                      }`}
                      onClick={() => {
                        setSoundCueVolume(value);
                        void invoke("set_setting", {
                          key: "sound_cue_volume",
                          value,
                        }).then(() => {
                          scheduleCloudSettingsPush();
                          void invoke("preview_sound_cue", { volume: value });
                        });
                      }}
                    >
                      {label}
                    </button>
                  ))}
                </div>
              ) : null}
              <Toggle
                checked={soundCue}
                onChange={() => {
                  const next = !soundCue;
                  setSoundCue(next);
                  void invoke("set_setting", {
                    key: "sound_cue",
                    value: next ? "true" : "false",
                  }).then(() => {
                    scheduleCloudSettingsPush();
                    if (next) {
                      void invoke("preview_sound_cue", {
                        volume: soundCueVolume,
                      });
                    }
                  });
                }}
                label="Start / stop sound"
              />
            </div>
          </div>
          <SettingsToggle
            title="Multilingual"
            desc={
              tierSupportsMultilingual(plan?.tier)
                ? "Code-switch between languages in one dictation (pick up to 5). Off = one language."
                : "Starter+ only. Free plan can use one language at a time."
            }
            checked={sttMultilingual && tierSupportsMultilingual(plan?.tier)}
            onChange={() => void toggleMultilingual()}
            disabled={!tierSupportsMultilingual(plan?.tier)}
          />
          <div className="settings-row settings-row-stack">
            <div className="settings-row-text">
              <div className="settings-row-title">
                {sttMultilingual && tierSupportsMultilingual(plan?.tier)
                  ? "Languages (up to 5)"
                  : "Language"}
              </div>
              <div className="settings-row-desc">
                {sttMultilingual && tierSupportsMultilingual(plan?.tier)
                  ? "Code-switch mid-sentence between supported languages. For clearest English-only dictation, turn Multilingual off and select English."
                  : "Speech recognition language for live dictation. Free plan: one language."}
              </div>
            </div>
            <div className="lang-chip-grid">
              {STT_LANGUAGES.map((lang) => {
                const multiOn =
                  sttMultilingual && tierSupportsMultilingual(plan?.tier);
                const active = sttLanguages.includes(lang.code);
                const lockedOut =
                  multiOn && !active && sttLanguages.length >= MAX_STT_LANGUAGES;
                return (
                  <button
                    key={lang.code}
                    type="button"
                    className={`lang-chip${active ? " is-active" : ""}${lockedOut ? " is-disabled" : ""}`}
                    disabled={lockedOut}
                    onClick={() => void toggleSttLanguage(lang.code)}
                    title={
                      lang.multi
                        ? `${lang.label} · code-switch ready`
                        : `${lang.label} · best as primary / single`
                    }
                  >
                    {lang.label}
                  </button>
                );
              })}
            </div>
          </div>
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
            desc="Start MaxSpeech in the background when you log in"
            checked={autostart}
            onChange={toggleAutostart}
          />
          {autostart && (
            <div className="settings-nested">
              <SettingsToggle
                title="Show window at login"
                desc="Off keeps MaxSpeech in the tray until you open it"
                checked={openOnLaunch}
                onChange={() =>
                  toggleBool(
                    "open_window_on_launch",
                    openOnLaunch,
                    setOpenOnLaunch,
                  )
                }
              />
            </div>
          )}
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
                  ? updatePct != null && updatePct >= 100
                    ? "Restarting…"
                    : updatePct != null
                      ? `Installing ${updatePct}%…`
                      : "Restarting…"
                  : "Update now"}
              </button>
            </div>
          )}

          {updateMsg && (
            <p className="px-4 pb-3 text-xs text-[var(--ms-text-dim)]">{updateMsg}</p>
          )}

          <div className="settings-row">
            <div className="settings-row-text">
              <div className="settings-row-title">Legal</div>
              <div className="settings-row-desc">
                Terms of Service and Privacy Policy
              </div>
            </div>
            <div className="flex gap-2 shrink-0">
              <button
                type="button"
                className="px-3 py-1.5 text-xs rounded-full text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)] transition-colors"
                style={{ background: "var(--ms-fill-muted)" }}
                onClick={() =>
                  void openUrl("https://maxspeech.vercel.app/terms.html")
                }
              >
                Terms
              </button>
              <button
                type="button"
                className="px-3 py-1.5 text-xs rounded-full text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)] transition-colors"
                style={{ background: "var(--ms-fill-muted)" }}
                onClick={() =>
                  void openUrl("https://maxspeech.vercel.app/privacy.html")
                }
              >
                Privacy
              </button>
            </div>
          </div>
          <p className="px-4 pb-3 text-[11px] text-[var(--ms-text-dim)] leading-relaxed">
            Remake keeps at most 10 recent recordings on this PC. Live dictation
            uses speech/AI APIs; MaxSpeech does not train models on your content.
          </p>
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
  disabled,
}: {
  title: string;
  desc: string;
  checked: boolean;
  onChange: () => void;
  disabled?: boolean;
}) {
  return (
    <div className={`settings-row${disabled ? " opacity-70" : ""}`}>
      <div className="settings-row-text">
        <div className="settings-row-title">{title}</div>
        <div className="settings-row-desc">{desc}</div>
      </div>
      <Toggle
        checked={checked}
        onChange={onChange}
        label={title}
        disabled={disabled}
      />
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
