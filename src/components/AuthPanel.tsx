import { useState } from "react";
import { signIn, signUp, type AuthUser } from "../lib/auth";

export default function AuthPanel({
  onAuthed,
}: {
  onAuthed: (user: AuthUser) => void;
}) {
  const [mode, setMode] = useState<"login" | "signup">("login");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [username, setUsername] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [info, setInfo] = useState("");

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (busy) return;
    setBusy(true);
    setError("");
    setInfo("");
    try {
      if (mode === "signup") {
        const { user, needsEmailConfirm } = await signUp(email, password, username);
        if (needsEmailConfirm) {
          setInfo("Check your email to confirm your account, then sign in.");
          setMode("login");
        } else {
          onAuthed(user);
        }
      } else {
        const user = await signIn(email, password);
        onAuthed(user);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="w-full max-w-md space-y-5">
      <div className="text-center space-y-2">
        <div className="ms-logo ms-logo--lg mx-auto">
          <img
            src="/logo.png"
            srcSet="/logo.png 1x, /logo@2x.png 2x"
            alt="MaxSpeech"
            width={72}
            height={72}
            draggable={false}
          />
        </div>
        <h1 className="text-3xl font-bold accent-gradient-text">
          {mode === "login" ? "Sign in" : "Create account"}
        </h1>
        <p className="text-sm text-[var(--ms-text-dim)]">
          Your account lives on the MaxSpeech cloud — separate from other Maximus apps.
        </p>
      </div>

      <form onSubmit={submit} className="space-y-3">
        {mode === "signup" && (
          <label className="block space-y-1.5">
            <span className="text-xs text-[var(--ms-text-dim)]">Username</span>
            <input
              type="text"
              autoComplete="username"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="yourname"
              className="w-full px-3 py-2.5 rounded-xl bg-[var(--ms-surface)] border border-[var(--ms-border)] text-sm outline-none focus:border-[var(--ms-turquoise)]"
              required
              minLength={2}
            />
          </label>
        )}
        <label className="block space-y-1.5">
          <span className="text-xs text-[var(--ms-text-dim)]">Email</span>
          <input
            type="email"
            autoComplete="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="you@email.com"
            className="w-full px-3 py-2.5 rounded-xl bg-[var(--ms-surface)] border border-[var(--ms-border)] text-sm outline-none focus:border-[var(--ms-turquoise)]"
            required
          />
        </label>
        <label className="block space-y-1.5">
          <span className="text-xs text-[var(--ms-text-dim)]">Password</span>
          <input
            type="password"
            autoComplete={mode === "login" ? "current-password" : "new-password"}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="••••••••"
            className="w-full px-3 py-2.5 rounded-xl bg-[var(--ms-surface)] border border-[var(--ms-border)] text-sm outline-none focus:border-[var(--ms-turquoise)]"
            required
            minLength={6}
          />
        </label>

        {error && <p className="text-sm text-[var(--ms-error)]">{error}</p>}
        {info && <p className="text-sm text-[var(--ms-turquoise)]">{info}</p>}

        <button
          type="submit"
          disabled={busy}
          className="btn-primary w-full px-4 py-2.5 text-sm disabled:opacity-50"
        >
          {busy ? "Please wait…" : mode === "login" ? "Sign in" : "Create account"}
        </button>
      </form>

      <p className="text-center text-sm text-[var(--ms-text-dim)]">
        {mode === "login" ? (
          <>
            No account?{" "}
            <button
              type="button"
              className="text-[var(--ms-turquoise)] hover:underline"
              onClick={() => {
                setMode("signup");
                setError("");
                setInfo("");
              }}
            >
              Sign up
            </button>
          </>
        ) : (
          <>
            Already have one?{" "}
            <button
              type="button"
              className="text-[var(--ms-turquoise)] hover:underline"
              onClick={() => {
                setMode("login");
                setError("");
                setInfo("");
              }}
            >
              Sign in
            </button>
          </>
        )}
      </p>
    </div>
  );
}
