import { invoke } from "@tauri-apps/api/core";
import { supabase } from "./supabase";
import type { PlanTier } from "./plan";

/** Local settings keys mirrored to Supabase `user_settings.settings`. */
export const SYNC_SETTING_KEYS = [
  "show_live_transcript",
  "ai_enhance",
  "trailing_space",
  "sound_cue",
  "ui_theme",
  "plan_tier",
] as const;

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

export async function pushCloudSettings(): Promise<void> {
  const { data: session } = await supabase.auth.getSession();
  const uid = session.session?.user?.id;
  if (!uid) return;

  const settings = await readLocalSettings();
  const tier = (settings.plan_tier || "free") as PlanTier;

  await supabase
    .from("profiles")
    .update({
      plan_tier: tier,
      updated_at: new Date().toISOString(),
    })
    .eq("id", uid);

  const { error } = await supabase.from("user_settings").upsert(
    {
      user_id: uid,
      settings,
      updated_at: new Date().toISOString(),
    },
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

  const settings = {
    ...((data?.settings as CloudSettings) || {}),
  };
  if (profile?.plan_tier) {
    settings.plan_tier = profile.plan_tier;
  }

  if (Object.keys(settings).length > 0) {
    await applyLocalSettings(settings);
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
    >("get_history", { search: "" });
    for (const row of rows.slice(0, 200)) {
      await pushHistoryIfMax(row);
    }
  } catch (e) {
    console.warn("syncAllLocalHistoryIfMax:", e);
  }
}
