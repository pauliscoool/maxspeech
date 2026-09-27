import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import ConfirmModal from "../../components/ConfirmModal";
import type { AuthUser } from "../../lib/auth";
import { signOut } from "../../lib/auth";
import { flushScheduledCloudSettingsPush } from "../../lib/cloudSync";
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
import { isOwnerAccount } from "../../lib/planAccess";

export default function ProfilePage({
  authUser,
  onChanged,
}: {
  authUser?: AuthUser | null;
  onChanged: () => void;
}) {
  const [firstName, setFirstName] = useState("");
  const [lastName, setLastName] = useState("");
  const [avatarUrl, setAvatarUrl] = useState<string | null>(null);
  const [profileMsg, setProfileMsg] = useState("");
  const [profileErr, setProfileErr] = useState("");
  const [savingNames, setSavingNames] = useState(false);
  const avatarInputRef = useRef<HTMLInputElement>(null);
  const profileLoadedRef = useRef(false);
  const savedNamesRef = useRef({ first: "", last: "" });
  const [userIdVisible, setUserIdVisible] = useState(false);
  const userIdHideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [logoutOpen, setLogoutOpen] = useState(false);
  const [loggingOut, setLoggingOut] = useState(false);

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
    return () => {
      if (userIdHideTimer.current) {
        clearTimeout(userIdHideTimer.current);
      }
    };
  }, []);

  function hideUserId() {
    if (userIdHideTimer.current) {
      clearTimeout(userIdHideTimer.current);
      userIdHideTimer.current = null;
    }
    setUserIdVisible(false);
  }

  function revealUserId() {
    if (userIdHideTimer.current) {
      clearTimeout(userIdHideTimer.current);
    }
    setUserIdVisible(true);
    userIdHideTimer.current = setTimeout(() => {
      setUserIdVisible(false);
      userIdHideTimer.current = null;
    }, 15_000);
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
        <h1 className="page-title">Profile</h1>
        <p className="page-subtitle">
          Name, photo, and account. Preferences live in Settings.
        </p>
      </header>

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
                  <div className="settings-row-title flex items-center gap-1.5">
                    {formatFullName(firstName, lastName) || authUser.username}
                    {isOwnerAccount(authUser.email) ? (
                      <span className="owner-tag">Owner</span>
                    ) : null}
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
              <div className="settings-row-text min-w-0">
                <div className="settings-row-title">User ID</div>
                {userIdVisible ? (
                  <div className="settings-row-desc font-mono text-[11px] break-all">
                    {authUser.id}
                  </div>
                ) : (
                  <div className="ms-id-mosaic" aria-hidden="true">
                    <span className="ms-id-mosaic-text">
                      xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
                    </span>
                  </div>
                )}
              </div>
              <button
                type="button"
                onClick={() => (userIdVisible ? hideUserId() : revealUserId())}
                className="px-3.5 py-1.5 text-xs rounded-full font-semibold shrink-0 transition-colors text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)]"
                style={{ background: "var(--ms-fill-muted)" }}
                aria-pressed={userIdVisible}
                aria-label={userIdVisible ? "Hide user ID" : "Show user ID for 15 seconds"}
              >
                {userIdVisible ? "Hide" : "Show"}
              </button>
            </div>
          </div>
        </section>
      )}

      <section className="space-y-2.5 pb-6">
        <h2 className="settings-section-title" style={{ color: "var(--ms-error)" }}>
          Session
        </h2>
        <div className="danger-zone">
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
