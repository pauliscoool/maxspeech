import { createClient } from "@supabase/supabase-js";

/** Dedicated MaxSpeech project — separate from Maximus-Dev. */
const SUPABASE_URL =
  import.meta.env.VITE_SUPABASE_URL ||
  "https://eqvmjmejcwkrylqyglfm.supabase.co";

const SUPABASE_ANON_KEY =
  import.meta.env.VITE_SUPABASE_ANON_KEY ||
  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZSIsInJlZiI6ImVxdm1qbWVqY3drcnlscXlnbGZtIiwicm9sZSI6ImFub24iLCJpYXQiOjE3ODU1MTA1ODMsImV4cCI6MjEwMTA4NjU4M30.9VauCDSWaAEpSuIkKnPsg9Ap7MLWtXqQSQ8I4xC7_fY";

export type Profile = {
  id: string;
  email: string;
  username: string;
  plan_tier: "free" | "starter" | "pro" | "max";
  created_at: string;
  updated_at: string;
};

export type UserSettingsRow = {
  user_id: string;
  settings: Record<string, string>;
  updated_at: string;
};

export const supabase = createClient(SUPABASE_URL, SUPABASE_ANON_KEY, {
  auth: {
    persistSession: true,
    autoRefreshToken: true,
    detectSessionInUrl: false,
  },
});
