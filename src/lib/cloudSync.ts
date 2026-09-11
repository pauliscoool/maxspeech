import { invoke } from "@tauri-apps/api/core";
import { supabase } from "./supabase";
import type { PlanTier } from "./plan";
import { showAccountSavedToast } from "./toast";

/** Local settings keys mirrored to Supabase `user_settings.settings`. */
export const SYNC_SETTING_KEYS = [
  "show_live_transcript",
  "ai_enhance",
  "enhance_speed",
  "trailing_space",
  "sound_cue",
  "sound_cue_volume",
  "ui_theme",
  "launch_at_startup",
  "open_window_on_launch",
  "plan_tier",
  "plan_updated_at",
  "usage_bonus",
  "usage_bonus_week",
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

// Collection keys stored as JSON strings inside user_settings.settings so
// dictionary / snippets / style overrides follow the account across devices.
const DICT_KEY = "dictionary_json";
const MACROS_KEY = "macros_json";
const PROFILES_KEY = "profiles_json";

async function readLocalCollections(): Promise<CloudSettings> {
  const out: CloudSettings = {};
  try {
    const dict = await invoke<{ word: string }[]>("get_dictionary");
    if (Array.isArray(dict)) out[DICT_KEY] = JSON.stringify(dict.map((d) => d.word));
  } catch {
    /* ignore — not in Tauri */
  }
  try {
    const macros = await invoke<{ trigger: string; expansion: string }[]>("get_macros");
    if (Array.isArray(macros)) {
      out[MACROS_KEY] = JSON.stringify(
        macros.map((m) => ({ trigger: m.trigger, expansion: m.expansion })),
      );
    }
  } catch {
    /* ignore */
  }
  try {
    const profiles = await invoke<
      { exe_pattern: string; title_pattern: string; tone: string; enabled: boolean }[]
    >("get_app_profiles");
    if (Array.isArray(profiles) && profiles.length > 0) {
      out[PROFILES_KEY] = JSON.stringify(
        profiles.map((p) => ({
          exe: p.exe_pattern,
          title: p.title_pattern,
          tone: p.tone,
          enabled: p.enabled,
        })),
      );
    }
  } catch {
    /* ignore */
  }
  return out;
}

async function applyLocalCollections(settings: CloudSettings): Promise<void> {
  if (settings[DICT_KEY] != null) {
    try {
      const target: string[] = JSON.parse(settings[DICT_KEY]);
      if (Array.isArray(target)) {
        const local = (await invoke<{ id: number; word: string }[]>(
          "get_dictionary",
        ).catch(() => [])) as { id: number; word: string }[];
        const localSet = new Set(local.map((w) => w.word));
        const targetSet = new Set(target);
        for (const w of target) {
          if (!localSet.has(w) && w.trim()) {
            try {
              await invoke("add_dict_word", { word: w });
            } catch {}
          }
        }
        for (const lw of local) {
          if (!targetSet.has(lw.word)) {
            try {
              await invoke("delete_dict_word", { id: lw.id });
            } catch {}
          }
        }
      }
    } catch {}
  }
  if (settings[MACROS_KEY] != null) {
    try {
      const target = JSON.parse(settings[MACROS_KEY]) as {
        trigger: string;
        expansion: string;
      }[];
      if (Array.isArray(target)) {
        const local = (await invoke<
          { id: number; trigger: string; expansion: string }[]
        >("get_macros").catch(() => [])) as {
          id: number;
          trigger: string;
          expansion: string;
        }[];
        const localByTrigger = new Map(local.map((m) => [m.trigger, m]));
        const targetByTrigger = new Map(target.map((m) => [m.trigger, m.expansion]));
        for (const t of target) {
          const existing = localByTrigger.get(t.trigger);
          if (!existing) {
            try {
              await invoke("add_macro", { trigger: t.trigger, expansion: t.expansion });
            } catch {}
          } else if (existing.expansion !== t.expansion) {
            try {
              await invoke("delete_macro", { id: existing.id });
              await invoke("add_macro", { trigger: t.trigger, expansion: t.expansion });
            } catch {}
          }
        }
        for (const lm of local) {
          if (!targetByTrigger.has(lm.trigger)) {
            try {
              await invoke("delete_macro", { id: lm.id });
            } catch {}
          }
        }
      }
    } catch {}
  }
  if (settings[PROFILES_KEY] != null) {
    try {
      const target = JSON.parse(settings[PROFILES_KEY]) as {
        exe: string;
        title: string;
        tone: string;
        enabled: boolean;
      }[];
      if (Array.isArray(target) && target.length > 0) {
        const local = (await invoke<
          {
            id: number;
            exe_pattern: string;
            title_pattern: string;
            tone: string;
            enabled: boolean;
          }[]
        >("get_app_profiles").catch(() => [])) as {
          id: number;
          exe_pattern: string;
          title_pattern: string;
          tone: string;
          enabled: boolean;
        }[];
        const byKey = new Map(
          local.map((p) => [
            `${p.exe_pattern.toLowerCase()}|${p.title_pattern.toLowerCase()}`,
            p,
          ]),
        );
        for (const t of target) {
          const key = `${String(t.exe).toLowerCase()}|${String(t.title).toLowerCase()}`;
          const match = byKey.get(key);
          if (match && (match.tone !== t.tone || match.enabled !== t.enabled)) {
            try {
              await invoke("update_app_profile", {
                id: match.id,
                tone: t.tone,
                enabled: t.enabled,
              });
            } catch {}
          }
        }
      }
    } catch {}
  }
}

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
  Object.assign(out, await readLocalCollections());
  return out;
}

async function applyLocalSettings(settings: CloudSettings): Promise<void> {
  for (const key of SYNC_SETTING_KEYS) {
    const v = settings[key];
    if (v == null) continue;
    try {
      if (key === "plan_updated_at") {
        continue;
      }
      if (key === "plan_tier") {
        await invoke("sync_plan_tier", { tier: v });
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
  await applyLocalCollections(settings);
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
    .select("plan_tier, updated_at")
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

  let localTier = "";
  let localAt = "";
  try {
    localTier = (await invoke<string>("get_setting", { key: "plan_tier" })) || "";
    localAt = (await invoke<string>("get_setting", { key: "plan_updated_at" })) || "";
  } catch {
    /* ignore */
  }

  const cloudTier = String(profile?.plan_tier || settings.plan_tier || "").toLowerCase();
  const cloudAt = String(profile?.updated_at || "");
  const chosen = pickPersistedPlan(localTier, localAt, cloudTier, cloudAt);
  if (chosen) settings.plan_tier = chosen;

  if (Object.keys(settings).length > 0) {
    const appliedAdmin = await applyAdminWordsUsed(settings);
    await applyLocalSettings(settings);
    if (appliedAdmin) {
      const cleaned = { ...settings };
      delete cleaned.admin_words_used;
      delete cleaned.admin_words_used_week;
      await supabase.from("user_settings").upsert(
        {
          user_id: uid,
          settings: cleaned,
          updated_at: new Date().toISOString(),
        },
        { onConflict: "user_id" },
      );
    }
  }
  if (typeof window !== "undefined") {
    window.dispatchEvent(new Event("maxspeech-profile-changed"));
    window.dispatchEvent(new Event("maxspeech-plan-changed"));
  }
  if (chosen && localTier && chosen === localTier && localTier !== cloudTier) {
    void pushCloudSettings();
  }
  return settings;
}

async function applyAdminWordsUsed(settings: CloudSettings): Promise<boolean> {
  const raw = settings.admin_words_used;
  if (raw == null || raw === "") return false;
  try {
    const plan = await invoke<{ week_starts_at: string }>("get_plan_status");
    const week = settings.admin_words_used_week || "";
    if (week && plan?.week_starts_at && week !== plan.week_starts_at) return false;
    const words = Math.max(0, Math.floor(Number(raw) || 0));
    await invoke("set_words_used", { words });
    return true;
  } catch {
    return false;
  }
}

function parseStamp(value: string): number {
  const n = Date.parse(value);
  return Number.isFinite(n) ? n : 0;
}

/** Keep the plan the user actually picked. Newer stamp wins; never let a stale
 *  cloud "free" wipe a local paid/owner selection that has no stamp yet. */
function pickPersistedPlan(
  localTier: string,
  localAt: string,
  cloudTier: string,
  cloudAt: string,
): string {
  const local = localTier.trim().toLowerCase();
  const cloud = cloudTier.trim().toLowerCase();
  if (!local && !cloud) return "";
  if (!cloud) return local;
  if (!local) return cloud;
  const lt = parseStamp(localAt);
  const ct = parseStamp(cloudAt);
  if (lt && ct) return lt >= ct ? local : cloud;
  if (lt && !ct) return local;
  if (!lt && ct) return cloud;
  if (local !== "free" && cloud === "free") return local;
  if (cloud !== "free" && local === "free") return cloud;
  return local || cloud;
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
