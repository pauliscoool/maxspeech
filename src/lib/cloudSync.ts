import { invoke } from "@tauri-apps/api/core";
import { supabase } from "./supabase";
import type { PlanTier } from "./plan";

/** Local settings keys mirrored to Supabase `user_settings.settings`. */
export const SYNC_SETTING_KEYS = [
  "show_live_transcript",
  "ai_enhance",
  "trailing_space",
  "sound_cue",
  "sound_cue_volume",
  "ui_theme",
  "launch_at_startup",
  "plan_tier",
  "stt_multilingual",
  "stt_languages",
  "mic_device",
] as const;

export type CloudSettings = Record<string, string>;

// Collection keys stored as JSON strings inside user_settings.settings.
const DICT_KEY = "dictionary_json";
const MACROS_KEY = "macros_json";
const PROFILES_KEY = "profiles_json";

async function readLocalCollections(): Promise<CloudSettings> {
  const out: CloudSettings = {};
  try {
    const dict = await invoke<{ word: string }[]>("get_dictionary");
    if (Array.isArray(dict)) out[DICT_KEY] = JSON.stringify(dict.map((d) => d.word));
  } catch {
    /* ignore - not in Tauri */
  }
  try {
    const macros = await invoke<{ trigger: string; expansion: string }[]>("get_macros");
    if (Array.isArray(macros))
      out[MACROS_KEY] = JSON.stringify(
        macros.map((m) => ({ trigger: m.trigger, expansion: m.expansion })),
      );
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
        const local = (await invoke<{ id: number; word: string }[]>("get_dictionary").catch(
          () => [],
        )) as { id: number; word: string }[];
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
        const local = (await invoke<{ id: number; trigger: string; expansion: string }[]>(
          "get_macros",
        ).catch(() => [])) as { id: number; trigger: string; expansion: string }[];
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
          { id: number; exe_pattern: string; title_pattern: string; tone: string; enabled: boolean }[]
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
    } catch {}
  }
  try {
    out.hotkey = await invoke<string>("get_hotkey");
  } catch {}
  try {
    out.hotkey_mode = await invoke<string>("get_hotkey_mode");
  } catch {}
  const collections = await readLocalCollections();
  Object.assign(out, collections);
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
        await invoke("set_microphone", { device: v });
      } else {
        await invoke("set_setting", { key, value: v });
      }
    } catch {}
  }
  if (settings.hotkey) {
    try {
      await invoke("set_hotkey", { shortcut: settings.hotkey });
    } catch {}
  }
  if (settings.hotkey_mode) {
    try {
      await invoke("set_hotkey_mode", { mode: settings.hotkey_mode });
    } catch {}
  }
  await applyLocalCollections(settings);
}

export async function pushCloudSettings(): Promise<void> {
  const { data: session } = await supabase.auth.getSession();
  const uid = session.session?.user?.id;
  if (!uid) return;
  const local = await readLocalSettings();
  const tier = (local.plan_tier || "free") as PlanTier;
  await supabase
    .from("profiles")
    .update({ plan_tier: tier, updated_at: new Date().toISOString() })
    .eq("id", uid);
  let merged: CloudSettings = { ...local };
  try {
    const { data: existing } = await supabase
      .from("user_settings")
      .select("settings")
      .eq("user_id", uid)
      .maybeSingle();
    const cloud = (existing?.settings as CloudSettings) || {};
    merged = { ...cloud, ...local };
  } catch {}
  const { error } = await supabase.from("user_settings").upsert(
    { user_id: uid, settings: merged, updated_at: new Date().toISOString() },
    { onConflict: "user_id" },
  );
  if (error) console.warn("pushCloudSettings:", error.message);
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
  const settings = { ...((data?.settings as CloudSettings) || {}) };
  if (profile?.plan_tier) settings.plan_tier = profile.plan_tier;
  if (Object.keys(settings).length > 0) await applyLocalSettings(settings);
  return settings;
}

export type HistoryPayload = {
  id: number;
  text: string;
  app_name: string;
  created_at?: string;
};

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
  if (error && !/duplicate|unique/i.test(error.message)) console.warn("pushHistoryIfMax:", error.message);
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
    const rows = await invoke<{ id: number; text: string; app_name: string; created_at: string }[]>(
      "get_history",
      { search: "", limit: 200, offset: 0 },
    );
    for (const row of rows) await pushHistoryIfMax(row);
  } catch (e) {
    console.warn("syncAllLocalHistoryIfMax:", e);
  }
}

export async function pullCloudHistoryIfMax(): Promise<void> {
  const { data: session } = await supabase.auth.getSession();
  const uid = session.session?.user?.id;
  if (!uid) return;
  const { data: profile } = await supabase
    .from("profiles")
    .select("plan_tier")
    .eq("id", uid)
    .maybeSingle();
  if (profile?.plan_tier !== "max") return;
}
