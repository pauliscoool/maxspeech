import { supabase, type Profile } from "./supabase";
import type { PlanTier } from "./plan";
import { pullCloudSettings, pushCloudSettings } from "./cloudSync";

export type AuthUser = {
  id: string;
  email: string;
  username: string;
  planTier: PlanTier;
  /** True when using offline/local mode (no Supabase session). */
  local?: boolean;
};

const LOCAL_SESSION_KEY = "maxspeech_local_session";
/** Never block the shell on a slow/offline Supabase refresh. */
const CLOUD_SESSION_TIMEOUT_MS = 8_000;

const LOCAL_USER: AuthUser = {
  id: "local-user",
  email: "local@maxspeech.app",
  username: "Local",
  planTier: "pro",
  local: true,
};

function usernameFromEmail(email: string): string {
  const part = email.split("@")[0]?.trim();
  return part && part.length > 0 ? part : "user";
}

function readLocalUser(): AuthUser | null {
  try {
    const raw = localStorage.getItem(LOCAL_SESSION_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as AuthUser;
    if (!parsed?.id || !parsed.local) return null;
    return { ...LOCAL_USER, ...parsed, local: true };
  } catch {
    return null;
  }
}

function writeLocalUser(user: AuthUser): void {
  try {
    localStorage.setItem(LOCAL_SESSION_KEY, JSON.stringify(user));
  } catch {
    /* ignore */
  }
}

function clearLocalUser(): void {
  try {
    localStorage.removeItem(LOCAL_SESSION_KEY);
  } catch {
    /* ignore */
  }
}

async function restoreFromDeviceStore(): Promise<AuthUser | null> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const unlocked = await invoke<string | null>("get_setting", {
      key: "dictation_unlocked",
    });
    const email = await invoke<string | null>("get_setting", { key: "account_email" });
    if (unlocked !== "true") return null;
    const cleanEmail = (email ?? "").trim().toLowerCase();
    if (!cleanEmail) return null;

    if (cleanEmail === LOCAL_USER.email) {
      writeLocalUser(LOCAL_USER);
      return LOCAL_USER;
    }

    const plan = await invoke<{ tier: PlanTier }>("get_plan_status");
    return {
      id: "device-session",
      email: cleanEmail,
      username: usernameFromEmail(cleanEmail),
      planTier: plan.tier ?? "free",
    };
  } catch {
    return null;
  }
}

/** Skip cloud auth — use this device only (settings/history stay local). */
export async function signInLocal(): Promise<AuthUser> {
  clearLocalUser();
  writeLocalUser(LOCAL_USER);
  await unlockDictation(LOCAL_USER.email);
  return LOCAL_USER;
}

export async function getSessionUser(): Promise<AuthUser | null> {
  // Local / SQLite session first — never block the window on a network refresh.
  const local = readLocalUser();
  if (local) {
    void unlockDictation(local.email);
    return local;
  }
  const restored = await restoreFromDeviceStore();
  if (restored) {
    void unlockDictation(restored.email);
    return restored;
  }

  const { data, error } = await Promise.race([
    supabase.auth.getSession(),
    new Promise<{ data: { session: null }; error: null }>((resolve) => {
      window.setTimeout(
        () => resolve({ data: { session: null }, error: null }),
        CLOUD_SESSION_TIMEOUT_MS,
      );
    }),
  ]);
  if (!error && data.session?.user) {
    clearLocalUser();
    const u = data.session.user;
    const profile = await ensureProfile(
      u.id,
      u.email ?? "",
      (u.user_metadata?.username as string | undefined) ?? null,
    );
    await unlockDictation(profile.email);
    return {
      id: profile.id,
      email: profile.email,
      username: profile.username,
      planTier: profile.plan_tier,
    };
  }

  return null;
}

export async function ensureProfile(
  id: string,
  email: string,
  usernameHint: string | null,
): Promise<Profile> {
  const { data: existing } = await supabase
    .from("profiles")
    .select("*")
    .eq("id", id)
    .maybeSingle();

  if (existing) {
    return existing as Profile;
  }

  const username = (usernameHint?.trim() || usernameFromEmail(email)).slice(0, 64);
  const { data, error } = await supabase
    .from("profiles")
    .upsert(
      {
        id,
        email,
        username,
        plan_tier: "free",
        updated_at: new Date().toISOString(),
      },
      { onConflict: "id" },
    )
    .select("*")
    .single();

  if (error) throw new Error(error.message);

  await supabase.from("user_settings").upsert(
    { user_id: id, settings: {}, updated_at: new Date().toISOString() },
    { onConflict: "user_id" },
  );

  return data as Profile;
}

export async function signUp(
  email: string,
  password: string,
  username: string,
): Promise<{ user: AuthUser; needsEmailConfirm: boolean }> {
  const cleanEmail = email.trim().toLowerCase();
  const cleanUser = username.trim().slice(0, 64) || usernameFromEmail(cleanEmail);

  const { data, error } = await supabase.auth.signUp({
    email: cleanEmail,
    password,
    options: {
      data: { username: cleanUser },
    },
  });
  if (error) throw new Error(error.message);
  if (!data.user) throw new Error("Sign up failed — no user returned.");

  // Existing accounts return a user with empty identities and no session.
  if (
    !data.session &&
    Array.isArray(data.user.identities) &&
    data.user.identities.length === 0
  ) {
    throw new Error("Account already exists — sign in instead.");
  }

  const needsEmailConfirm = !data.session;
  if (!data.session) {
    return {
      user: {
        id: data.user.id,
        email: cleanEmail,
        username: cleanUser,
        planTier: "free",
      },
      needsEmailConfirm: true,
    };
  }

  const profile = await ensureProfile(data.user.id, cleanEmail, cleanUser);
  await unlockDictation(profile.email);
  await pushCloudSettings();
  return {
    user: {
      id: profile.id,
      email: profile.email,
      username: profile.username,
      planTier: profile.plan_tier,
    },
    needsEmailConfirm,
  };
}

export async function signIn(
  email: string,
  password: string,
): Promise<AuthUser> {
  const { data, error } = await supabase.auth.signInWithPassword({
    email: email.trim().toLowerCase(),
    password,
  });
  if (error) {
    const msg = error.message.toLowerCase();
    if (msg.includes("invalid login") || msg.includes("invalid credentials")) {
      throw new Error("Wrong email or password. Or use Continue locally below.");
    }
    throw new Error(error.message);
  }
  if (!data.user) throw new Error("Sign in failed.");

  clearLocalUser();
  const profile = await ensureProfile(
    data.user.id,
    data.user.email ?? email,
    (data.user.user_metadata?.username as string | undefined) ?? null,
  );
  await unlockDictation(profile.email);
  await pullCloudSettings();
  return {
    id: profile.id,
    email: profile.email,
    username: profile.username,
    planTier: profile.plan_tier,
  };
}

export async function signOut(): Promise<void> {
  clearLocalUser();
  await lockDictation();
  await supabase.auth.signOut();
}

async function persistAccountEmail(email: string): Promise<void> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("set_setting", {
      key: "account_email",
      value: email.trim().toLowerCase(),
    });
  } catch {
    /* ignore when not in Tauri */
  }
}

/** Unlock (or lock) the Rust hotkey gate. Login screen must call lockDictation(). */
export async function unlockDictation(email: string): Promise<void> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await persistAccountEmail(email);
    await invoke("set_setting", { key: "dictation_unlocked", value: "true" });
  } catch {
    /* ignore */
  }
}

export async function lockDictation(): Promise<void> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("set_setting", { key: "dictation_unlocked", value: "false" });
    await invoke("set_setting", { key: "account_email", value: "" });
  } catch {
    /* ignore */
  }
}

export async function updateCloudPlan(tier: PlanTier): Promise<void> {
  const { data: session } = await supabase.auth.getSession();
  const uid = session.session?.user?.id;
  if (!uid) return;
  await supabase
    .from("profiles")
    .update({ plan_tier: tier, updated_at: new Date().toISOString() })
    .eq("id", uid);
}

export function onAuthChange(cb: (user: AuthUser | null) => void): () => void {
  const { data } = supabase.auth.onAuthStateChange((_event, session) => {
    void (async () => {
      if (!session?.user) {
        const local = readLocalUser();
        if (local) {
          await unlockDictation(local.email);
          cb(local);
          return;
        }
        const restored = await restoreFromDeviceStore();
        if (restored) {
          await unlockDictation(restored.email);
          cb(restored);
          return;
        }
        await lockDictation();
        cb(null);
        return;
      }
      try {
        clearLocalUser();
        const profile = await ensureProfile(
          session.user.id,
          session.user.email ?? "",
          (session.user.user_metadata?.username as string | undefined) ?? null,
        );
        await unlockDictation(profile.email);
        cb({
          id: profile.id,
          email: profile.email,
          username: profile.username,
          planTier: profile.plan_tier,
        });
      } catch {
        const local = readLocalUser();
        if (local) {
          await unlockDictation(local.email);
          cb(local);
        } else {
          await lockDictation();
          cb(null);
        }
      }
    })();
  });
  return () => data.subscription.unsubscribe();
}
