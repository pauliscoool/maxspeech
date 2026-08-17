import { invoke } from "@tauri-apps/api/core";
import { supabase } from "./supabase";
import { scheduleCloudSettingsPush } from "./cloudSync";

export const PROFILE_CHANGED_EVENT = "maxspeech-profile-changed";
export const PROFILE_AVATAR_SAVED_EVENT = "maxspeech-profile-avatar-saved";

const AVATAR_SAVE_DELAY_MS = 3000;

export const MAX_AVATAR_BYTES = 1 * 1024 * 1024;

export const SETTING_FIRST_NAME = "profile_first_name";
export const SETTING_LAST_NAME = "profile_last_name";
export const SETTING_AVATAR = "profile_avatar";

const AVATAR_TYPES = new Set([
  "image/jpeg",
  "image/jpg",
  "image/png",
  "image/webp",
  "image/gif",
]);
const AVATAR_EXT = /\.(jpe?g|png|webp|gif)$/i;

export type ProfileIdentity = {
  firstName: string;
  lastName: string;
  email: string;
  avatarDataUrl: string | null;
};

export function notifyProfileChanged() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new Event(PROFILE_CHANGED_EVENT));
}

function notifyAvatarSaved() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new Event(PROFILE_AVATAR_SAVED_EVENT));
}

/** In-memory preview while the 3s persist debounce is pending. */
let pendingAvatarDataUrl: string | null = null;
let avatarSaveTimer: ReturnType<typeof setTimeout> | null = null;
let avatarSaveGeneration = 0;
let beforeUnloadBound = false;

function bindAvatarFlushOnLeave() {
  if (beforeUnloadBound || typeof window === "undefined") return;
  beforeUnloadBound = true;
  window.addEventListener("beforeunload", () => {
    void flushScheduledAvatarSave();
  });
}

function capitalize(part: string): string {
  if (!part) return part;
  return part.charAt(0).toUpperCase() + part.slice(1);
}

/** First/last from a display name, then email prefix (`jane.doe` → Jane / Doe). */
export function parseNameParts(
  displayName: string | null | undefined,
  email: string | null | undefined,
): { first: string; last: string } {
  const cleaned = (displayName ?? "").trim();
  if (cleaned) {
    const parts = cleaned.split(/\s+/);
    if (parts.length === 1) return { first: parts[0], last: "" };
    return { first: parts[0], last: parts.slice(1).join(" ") };
  }
  const prefix = (email ?? "").split("@")[0]?.trim() ?? "";
  if (!prefix) return { first: "Local", last: "" };
  const bits = prefix.split(/[._-]+/).filter(Boolean);
  if (bits.length >= 2) {
    return {
      first: capitalize(bits[0]),
      last: bits.slice(1).map(capitalize).join(" "),
    };
  }
  return { first: capitalize(prefix), last: "" };
}

export function formatFullName(first: string, last: string): string {
  return [first, last]
    .map((s) => s.trim())
    .filter(Boolean)
    .join(" ");
}

export function profileInitials(first: string, last: string): string {
  const a = first.trim().charAt(0);
  const b = last.trim().charAt(0);
  const s = `${a}${b}`.toUpperCase();
  return s || "MS";
}

export function validateAvatarFile(file: File): string | null {
  if (file.size > MAX_AVATAR_BYTES) {
    return "Images must be 1 MB or smaller.";
  }
  const typeOk = AVATAR_TYPES.has(file.type.toLowerCase());
  const extOk = AVATAR_EXT.test(file.name);
  if (!typeOk && !extOk) {
    return "Use a JPEG, PNG, WebP, or GIF image.";
  }
  return null;
}

function fileToDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result;
      if (typeof result === "string" && result.startsWith("data:image/")) {
        resolve(result);
        return;
      }
      reject(new Error("Could not read that image."));
    };
    reader.onerror = () => reject(new Error("Could not read that image."));
    reader.readAsDataURL(file);
  });
}

export async function loadProfileIdentity(fallback: {
  username?: string | null;
  email?: string | null;
}): Promise<ProfileIdentity> {
  let first = "";
  let last = "";
  let avatar: string | null = null;
  try {
    first = (await invoke<string>("get_setting", { key: SETTING_FIRST_NAME })).trim();
    last = (await invoke<string>("get_setting", { key: SETTING_LAST_NAME })).trim();
    const raw = (await invoke<string>("get_setting", { key: SETTING_AVATAR })).trim();
    avatar = pendingAvatarDataUrl ?? (raw.startsWith("data:image/") ? raw : null);
  } catch {
    /* ignore when not in Tauri */
  }
  if (pendingAvatarDataUrl) avatar = pendingAvatarDataUrl;

  if (!first && !last) {
    try {
      const { data } = await supabase.auth.getSession();
      const meta = data.session?.user?.user_metadata ?? {};
      first = String(meta.first_name ?? "").trim();
      last = String(meta.last_name ?? "").trim();
    } catch {
      /* ignore */
    }
  }

  if (!first && !last) {
    const parsed = parseNameParts(fallback.username, fallback.email);
    first = parsed.first;
    last = parsed.last;
  }

  return {
    firstName: first,
    lastName: last,
    email: (fallback.email ?? "").trim(),
    avatarDataUrl: avatar,
  };
}

async function persistCloudNames(first: string, last: string, display: string) {
  const { data } = await supabase.auth.getSession();
  const uid = data.session?.user?.id;
  if (!uid) return;
  const username = (display || first).slice(0, 64);
  await supabase
    .from("profiles")
    .update({
      username,
      updated_at: new Date().toISOString(),
    })
    .eq("id", uid);
  await supabase.auth.updateUser({
    data: { first_name: first, last_name: last, username },
  });
}

export async function saveProfileNames(firstName: string, lastName: string): Promise<void> {
  const first = firstName.trim().slice(0, 64);
  const last = lastName.trim().slice(0, 64);
  await invoke("set_setting", { key: SETTING_FIRST_NAME, value: first });
  await invoke("set_setting", { key: SETTING_LAST_NAME, value: last });
  const display = formatFullName(first, last);
  try {
    await invoke("set_setting", { key: "user_name", value: display });
  } catch {
    /* ignore */
  }
  await persistCloudNames(first, last, display);
  scheduleCloudSettingsPush();
  notifyProfileChanged();
}

export function cancelScheduledAvatarSave() {
  if (avatarSaveTimer) {
    clearTimeout(avatarSaveTimer);
    avatarSaveTimer = null;
  }
  avatarSaveGeneration += 1;
  pendingAvatarDataUrl = null;
}

/** Persist the latest pending photo now (used on app close). */
export async function flushScheduledAvatarSave(): Promise<void> {
  if (avatarSaveTimer) {
    clearTimeout(avatarSaveTimer);
    avatarSaveTimer = null;
  }
  if (!pendingAvatarDataUrl) return;
  await persistPendingAvatar(avatarSaveGeneration);
}

async function persistPendingAvatar(generation: number): Promise<void> {
  if (generation !== avatarSaveGeneration) return;
  const dataUrl = pendingAvatarDataUrl;
  if (!dataUrl) return;
  try {
    await invoke("set_setting", { key: SETTING_AVATAR, value: dataUrl });
    scheduleCloudSettingsPush();
    if (generation !== avatarSaveGeneration) return;
    if (pendingAvatarDataUrl === dataUrl) pendingAvatarDataUrl = null;
    notifyProfileChanged();
    notifyAvatarSaved();
  } catch (e) {
    console.warn("persistPendingAvatar:", e);
  }
}

/**
 * Preview immediately; write SQLite + cloud only after ~3s of no new pick.
 * Rejects over 1 MB before the timer starts. Picking again resets the timer.
 */
export async function scheduleProfileAvatarSave(file: File): Promise<string> {
  const err = validateAvatarFile(file);
  if (err) throw new Error(err);

  const generation = ++avatarSaveGeneration;
  if (avatarSaveTimer) {
    clearTimeout(avatarSaveTimer);
    avatarSaveTimer = null;
  }

  const dataUrl = await fileToDataUrl(file);
  if (generation !== avatarSaveGeneration) {
    throw new DOMException("A newer photo was selected.", "AbortError");
  }

  pendingAvatarDataUrl = dataUrl;
  bindAvatarFlushOnLeave();
  notifyProfileChanged();

  avatarSaveTimer = setTimeout(() => {
    avatarSaveTimer = null;
    void persistPendingAvatar(generation);
  }, AVATAR_SAVE_DELAY_MS);

  return dataUrl;
}
