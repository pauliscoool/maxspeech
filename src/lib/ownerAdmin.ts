import { supabase } from "./supabase";
import type { PlanTier } from "./plan";
import { isOwnerAccount } from "./planAccess";

export type CloudPerson = {
  id: string;
  email: string;
  username: string;
  plan_tier: PlanTier;
  usage_bonus: number;
  words_used: number;
  updated_at: string;
};

function asSettings(settings: unknown): Record<string, string> {
  if (!settings || typeof settings !== "object") return {};
  return settings as Record<string, string>;
}

function bonusFromSettings(settings: unknown): number {
  const rec = asSettings(settings);
  const bonus = Number(rec.usage_bonus || 0);
  if (!Number.isFinite(bonus) || bonus <= 0) return 0;
  return Math.floor(bonus);
}

function wordsUsedFromSettings(settings: unknown): number {
  const rec = asSettings(settings);
  const used = Number(rec.admin_words_used ?? rec.words_used ?? 0);
  if (!Number.isFinite(used) || used < 0) return 0;
  return Math.floor(used);
}

function toPerson(
  row: {
    id: string;
    email: string;
    username: string;
    plan_tier: PlanTier;
    updated_at: string;
  },
  settings: unknown,
): CloudPerson {
  return {
    ...row,
    usage_bonus: bonusFromSettings(settings),
    words_used: wordsUsedFromSettings(settings),
  };
}

async function mergeUserSettings(
  userId: string,
  patch: Record<string, string>,
): Promise<void> {
  const { data: existing, error: readErr } = await supabase
    .from("user_settings")
    .select("settings")
    .eq("user_id", userId)
    .maybeSingle();
  if (readErr) throw new Error(readErr.message);
  const settings = {
    ...asSettings(existing?.settings),
    ...patch,
  };
  const { error } = await supabase.from("user_settings").upsert(
    {
      user_id: userId,
      settings,
      updated_at: new Date().toISOString(),
    },
    { onConflict: "user_id" },
  );
  if (error) throw new Error(error.message);
}

export async function listCloudPeople(
  ownerEmail: string | null | undefined,
): Promise<CloudPerson[]> {
  if (!isOwnerAccount(ownerEmail)) return [];
  const { data, error } = await supabase
    .from("profiles")
    .select("id, email, username, plan_tier, updated_at")
    .order("email");
  if (error) throw new Error(error.message);
  const rows = (data ?? []) as {
    id: string;
    email: string;
    username: string;
    plan_tier: PlanTier;
    updated_at: string;
  }[];
  const { data: settingsRows } = await supabase
    .from("user_settings")
    .select("user_id, settings");
  const settingsByUser = new Map<string, unknown>();
  for (const row of settingsRows ?? []) {
    settingsByUser.set(row.user_id as string, row.settings);
  }
  return rows.map((r) => toPerson(r, settingsByUser.get(r.id)));
}

export async function getCloudPerson(
  ownerEmail: string | null | undefined,
  userId: string,
): Promise<CloudPerson> {
  if (!isOwnerAccount(ownerEmail)) throw new Error("Owner only");
  const id = userId.trim();
  if (!id) throw new Error("Enter a user ID");
  const { data, error } = await supabase
    .from("profiles")
    .select("id, email, username, plan_tier, updated_at")
    .eq("id", id)
    .maybeSingle();
  if (error) throw new Error(error.message);
  if (!data) throw new Error("No account with that user ID");
  const { data: settingsRow, error: setErr } = await supabase
    .from("user_settings")
    .select("settings")
    .eq("user_id", id)
    .maybeSingle();
  if (setErr) throw new Error(setErr.message);
  return toPerson(
    data as {
      id: string;
      email: string;
      username: string;
      plan_tier: PlanTier;
      updated_at: string;
    },
    settingsRow?.settings,
  );
}

export async function setCloudPersonPlan(
  ownerEmail: string | null | undefined,
  userId: string,
  tier: PlanTier,
): Promise<void> {
  if (!isOwnerAccount(ownerEmail)) throw new Error("Owner only");
  const { error } = await supabase
    .from("profiles")
    .update({ plan_tier: tier, updated_at: new Date().toISOString() })
    .eq("id", userId);
  if (error) throw new Error(error.message);
}

export async function setCloudPersonBonus(
  ownerEmail: string | null | undefined,
  userId: string,
  bonus: number,
  weekStart: string,
): Promise<void> {
  if (!isOwnerAccount(ownerEmail)) throw new Error("Owner only");
  await mergeUserSettings(userId, {
    usage_bonus: String(Math.max(0, Math.floor(bonus))),
    usage_bonus_week: weekStart,
  });
}

export async function setCloudPersonWordsUsed(
  ownerEmail: string | null | undefined,
  userId: string,
  words: number,
  weekStart: string,
): Promise<void> {
  if (!isOwnerAccount(ownerEmail)) throw new Error("Owner only");
  await mergeUserSettings(userId, {
    admin_words_used: String(Math.max(0, Math.floor(words))),
    admin_words_used_week: weekStart,
  });
}
