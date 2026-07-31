import { supabase, type Profile } from "./supabase";
import type { PlanTier } from "./plan";
import { pullCloudSettings, pushCloudSettings } from "./cloudSync";

export type AuthUser = {
  id: string;
  email: string;
  username: string;
  planTier: PlanTier;
};

function usernameFromEmail(email: string): string {
  const part = email.split("@")[0]?.trim();
  return part && part.length > 0 ? part : "user";
}

export async function getSessionUser(): Promise<AuthUser | null> {
  const { data, error } = await supabase.auth.getSession();
  if (error || !data.session?.user) return null;
  const u = data.session.user;
  const profile = await ensureProfile(
    u.id,
    u.email ?? "",
    (u.user_metadata?.username as string | undefined) ?? null,
  );
  return {
    id: profile.id,
    email: profile.email,
    username: profile.username,
    planTier: profile.plan_tier,
  };
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
  if (error) throw new Error(error.message);
  if (!data.user) throw new Error("Sign in failed.");

  const profile = await ensureProfile(
    data.user.id,
    data.user.email ?? email,
    (data.user.user_metadata?.username as string | undefined) ?? null,
  );
  await pullCloudSettings();
  return {
    id: profile.id,
    email: profile.email,
    username: profile.username,
    planTier: profile.plan_tier,
  };
}

export async function signOut(): Promise<void> {
  await supabase.auth.signOut();
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
        cb(null);
        return;
      }
      try {
        const profile = await ensureProfile(
          session.user.id,
          session.user.email ?? "",
          (session.user.user_metadata?.username as string | undefined) ?? null,
        );
        cb({
          id: profile.id,
          email: profile.email,
          username: profile.username,
          planTier: profile.plan_tier,
        });
      } catch {
        cb(null);
      }
    })();
  });
  return () => data.subscription.unsubscribe();
}
