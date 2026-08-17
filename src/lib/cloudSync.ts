import { invoke } from "@tauri-apps/api/core";
import { supabase } from "./supabase";
import type { PlanTier } from "./plan";
import { showAccountSavedToast } from "./toast";

/** Local settings keys mirrored to Supabase `user_settings.settings`. */
export const SYNC_SETTING_KEYS = [
  "show_live_transcript",
  "ai_enhance",
  "trailing_space",
  "sound_cue",
  "sound_cue_volume",
  "ui_theme",
  "launch_at_startup",
  "open_window_on_launch",
  "plan_tier",
  "stt_multilingual",
  "stt_languages",
  "mic_device",
  "profile_first_name",
  "profile_last_name",
  "profile_avatar",
] as const;

/** Debounce from last settings edit before pushing to the account. */
export const ACCOUNT_SAVE_DEBOUNCE_MS = 5000;

export type CloudSettings = Record<string, string>;

async function readLocalSettings(): Promise<CloudSettings> {
  const out: CloudSettings = {};
  for (const key of SYNC_SETTING_KEYS) {
    try {
      const v = await invoke<string | null>("get_setting", { key });
      if (v != null && v !== "") out[key] = v;
    } catch {
      // ignore missing
    }
  }
  try {
    out.hotkey = await invoke<string>("get_hotkey");
  } catch {
    /* ignore */
  }
  try {
    out.hotkey_mode = await invoke<string>("get_hotkey_mode");
  } catch {
    /* ignore */
  }
  return out;
}

async function applyLocalSettings(settings: CloudSettings): Promise<void> {
  for (const key of SYNC_SETTING_KEYS) {
    const v = settings[key];
    if (v == null) continue;
    try {
      if (key === "plan_tier") {
        await invoke("set_plan_tier", { tier: v });
      } else if (key === "mic_device") {
        // Validate / fuzzy-match against currently attached devices.
        await invoke("set_microphone", { device: v });
      } else {
        await invoke("set_setting", { key, value: v });
      }
    } catch {
      /* ignore */
    }
  }
  if (settings.hotkey) {
    try {
      await invoke("set_hotkey", { shortcut: settings.hotkey });
    } catch {
      /* ignore */
    }
  }
  if (settings.hotkey_mode) {
    try {
      await invoke("set_hotkey_mode", { mode: settings.hotkey_mode });
    } catch {
      /* ignore */
    }
  }
}

export async function hasCloudSession(): Promise<boolean> {
  const { data: session } = await supabase.auth.getSession();
  return Boolean(session.session?.user?.id);
}

/**
 * Push local prefs (hotkey, theme, mic, …) to Supabase.
 * No-ops for local-only / unsigned-in sessions. Returns true only when
 * the upsert succeeded for a real cloud user.
 */
export async function pushCloudSettings(): Promise<boolean> {
  const { data: session } = await supabase.auth.getSession();
  const uid = session.session?.user?.id;
  if (!uid) return false;

  const local = await readLocalSettings();
  const tier = (local.plan_tier || "free") as PlanTier;

  await supabase
    .from("profiles")
    .update({
      plan_tier: tier,
      updated_at: new Date().toISOString(),
    })
    .eq("id", uid);

  // Merge so a hotkey/theme push cannot wipe a stored avatar.
  const { data: existing } = await supabase
    .from("user_settings")
    .select("settings")
    .eq("user_id", uid)
    .maybeSingle();
  const settings: CloudSettings = {
    ...((existing?.settings as CloudSettings) || {}),
    ...local,
  };

  const { error } = await supabase.from("user_settings").upsert(
    {
      user_id: uid,
      settings,
      updated_at: new Date().toISOString(),
    },
    { onConflict: "user_id" },
  );
  if (error) {
    console.warn("pushCloudSettings:", error.message);
    return false;
  }
  return true;
}

let saveTimer: ReturnType<typeof setTimeout> | null = null;
let savePending = false;
let saveChain: Promise<void> = Promise.resolve();
let flushListenersBound = false;

function bindSaveFlushListeners() {
  if (flushListenersBound || typeof window === "undefined") return;
  flushListenersBound = true;
  const flushQuiet = () => {
    void flushScheduledCloudSettingsPush({ toast: false });
  };
  window.addEventListener("pagehide", flushQuiet);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") flushQuiet();
  });
}

/**
 * Debounce account sync: local SQLite already saved; wait ~5s after the last
 * edit, then push once and toast. Local-only users skip this (no account toast).
 */
export function scheduleCloudSettingsPush(): void {
  bindSaveFlushListeners();
  savePending = true;
  if (saveTimer != null) {
    clearTimeout(saveTimer);
  }
  saveTimer = setTimeout(() => {
    saveTimer = null;
    void commitScheduledCloudSettings({ toast: true });
  }, ACCOUNT_SAVE_DEBOUNCE_MS);
}

/** Flush a pending debounced push (e.g. window hidden). */
export function flushScheduledCloudSettingsPush(
  opts?: { toast?: boolean },
): Promise<void> {
  if (saveTimer != null) {
    clearTimeout(saveTimer);
    saveTimer = null;
  }
  return commitScheduledCloudSettings({ toast: opts?.toast === true });
}

function commitScheduledCloudSettings(opts: { toast: boolean }): Promise<void> {
  saveChain = saveChain
    .then(async () => {
      if (!savePending) return;
      savePending = false;
      const ok = await pushCloudSettings();
      // More edits arrived while we were pushing — let the next timer/flush toast.
      if (savePending) return;
      if (ok && opts.toast) showAccountSavedToast();
    })
    .catch((e) => {
      console.warn("scheduleCloudSettingsPush:", e);
    });
  return saveChain;
}

export async function pullCloudSettings(): Promise<CloudSettings | null> {
  const { data: session } = await supabase.auth.getSession();
  const uid = session.session?.user?.id;
  if (!uid) return null;

  const { data: profile } = await supabase
    .from("profiles")
    .select("plan_tier")
    .eq("id", uid)
    .maybeSingle();

  const { data, error } = await supabase
    .from("user_settings")
    .select("settings")
    .eq("user_id", uid)
    .maybeSingle();

  if (error) {
    console.warn("pullCloudSettings:", error.message);
    return null;
  }

  const settings = {
    ...((data?.settings as CloudSettings) || {}),
  };
  if (profile?.plan_tier) {
    settings.plan_tier = profile.plan_tier;
  }

  if (Object.keys(settings).length > 0) {
    await applyLocalSettings(settings);
  }
  if (typeof window !== "undefined") {
    window.dispatchEvent(new Event("maxspeech-profile-changed"));
  }
  return settings;
}

export type HistoryPayload = {
  id: number;
  text: string;
  app_name: string;
  created_at?: string;
};

/** Cloud history sync — Max plan only (also enforced by RLS). */
export async function pushHistoryIfMax(entry: HistoryPayload): Promise<void> {
  const { data: session } = await supabase.auth.getSession();
  const uid = session.session?.user?.id;
  if (!uid) return;

  const { data: profile } = await supabase
    .from("profiles")
    .select("plan_tier")
    .eq("id", uid)
    .maybeSingle();

  if (profile?.plan_tier !== "max") return;

  const { error } = await supabase.from("dictation_history").insert({
    user_id: uid,
    local_id: entry.id,
    text: entry.text,
    app_name: entry.app_name,
    created_at: entry.created_at || new Date().toISOString(),
  });
  if (error && !/duplicate|unique/i.test(error.message)) {
    console.warn("pushHistoryIfMax:", error.message);
  }
}

export async function syncAllLocalHistoryIfMax(): Promise<void> {
  const { data: session } = await supabase.auth.getSession();
  const uid = session.session?.user?.id;
  if (!uid) return;

  const { data: profile } = await supabase
    .from("profiles")
    .select("plan_tier")
    .eq("id", uid)
    .maybeSingle();
  if (profile?.plan_tier !== "max") return;

  try {
    const rows = await invoke<
      { id: number; text: string; app_name: string; created_at: string }[]
    >("get_history", { search: "", limit: 200, offset: 0 });
    for (const row of rows) {
      await pushHistoryIfMax(row);
    }
  } catch (e) {
    console.warn("syncAllLocalHistoryIfMax:", e);
  }
}
