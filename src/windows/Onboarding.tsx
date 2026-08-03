import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import AuthPanel from "../components/AuthPanel";
import type { AuthUser } from "../lib/auth";
import { defaultHotkey, formatHotkey } from "../lib/platform";

type Step = "welcome" | "account" | "mic" | "hotkey" | "done";

type MicInfo = { name: string; is_default: boolean };

export default function Onboarding() {
  const [step, setStep] = useState<Step>("welcome");
  const [micOk, setMicOk] = useState(false);
  const [micError, setMicError] = useState("");
  const [micLabel, setMicLabel] = useState("");
  const [testing, setTesting] = useState(false);
  const [user, setUser] = useState<AuthUser | null>(null);
  const [micDevices, setMicDevices] = useState<MicInfo[]>([]);
  const [micDevice, setMicDevice] = useState("default");
  const hotkeyLabel = formatHotkey(defaultHotkey());

  useEffect(() => {
    if (step !== "mic") return;
    void (async () => {
      try {
        const devices = await invoke<MicInfo[]>("list_microphones");
        setMicDevices(devices);
        const saved = await invoke<string>("get_microphone");
        setMicDevice(!saved || saved === "default" ? "default" : saved);
      } catch {
        setMicDevices([]);
      }
    })();
  }, [step]);

  async function applyMic(device: string) {
    const next = device || "default";
    setMicDevice(next);
    setMicOk(false);
    setMicError("");
    setMicLabel("");
    try {
      await invoke("set_microphone", { device: next });
    } catch (e) {
      setMicError(String(e));
    }
  }

  async function testMic() {
    setTesting(true);
    setMicError("");
    try {
      const result = await invoke<{
        name: string;
        ok: boolean;
        message: string;
      }>("test_microphone");
      setMicOk(result.ok);
      setMicLabel(result.name);
      if (!result.ok) setMicError(result.message);
    } catch (e) {
      setMicOk(false);
      setMicLabel("");
      setMicError(String(e));
    } finally {
      setTesting(false);
    }
  }

  async function finish() {
    await invoke("complete_onboarding");
  }

  function onAuthed(u: AuthUser) {
    setUser(u);
    setStep("mic");
  }

  return (
    <div className="flex items-center justify-center h-screen bg-[var(--ms-bg)] text-[var(--ms-text)] relative overflow-hidden">
      <div
        className="pointer-events-none absolute -left-24 bottom-0 w-72 h-72 rounded-full blur-3xl opacity-30"
        style={{ background: "radial-gradient(circle, var(--ms-orange), transparent 70%)" }}
      />
      <div
        className="pointer-events-none absolute -right-20 top-0 w-80 h-80 rounded-full blur-3xl opacity-25"
        style={{ background: "radial-gradient(circle, var(--ms-turquoise), transparent 70%)" }}
      />

      <div className="w-full max-w-md p-8 page-enter relative z-10" key={step}>
        {step === "welcome" && (
          <div className="text-center space-y-6">
            <div className="ms-logo ms-logo--lg">
              <img
                src="/logo.png"
                srcSet="/logo.png 1x, /logo@2x.png 2x"
                alt="MaxSpeech"
                width={72}
                height={72}
                draggable={false}
              />
            </div>
            <div className="text-5xl font-bold accent-gradient-text">MaxSpeech</div>
            <p className="text-[var(--ms-text-dim)]">
              AI dictation that types anywhere. Hold a hotkey, speak, release.
            </p>
            <button onClick={() => setStep("account")} className="btn-primary px-6 py-3 text-sm">
              Get Started
            </button>
          </div>
        )}

        {step === "account" && <AuthPanel onAuthed={onAuthed} />}

        {step === "mic" && (
          <div className="space-y-6">
            <h2 className="text-2xl font-semibold">Microphone</h2>
            {user && (
              <p className="text-xs text-[var(--ms-turquoise)]">
                Signed in as {user.username} ({user.email})
              </p>
            )}
            <p className="text-sm text-[var(--ms-text-dim)]">
              Defaults to your system microphone. Pick a different one if MaxSpeech is hearing the
              wrong device — your choice is saved.
            </p>
            <select
              value={micDevice}
              onChange={(e) => void applyMic(e.target.value)}
              className="input-field w-full px-3 py-2.5 text-sm"
              aria-label="Microphone input device"
            >
              <option value="default">
                System default
                {micDevices.find((d) => d.is_default)
                  ? ` — ${micDevices.find((d) => d.is_default)!.name}`
                  : ""}
              </option>
              {micDevices.map((d) => (
                <option key={d.name} value={d.name}>
                  {d.name}
                  {d.is_default ? " (system default)" : ""}
                </option>
              ))}
            </select>
            <button
              onClick={() => void testMic()}
              disabled={testing}
              className="px-4 py-2.5 rounded-xl bg-[var(--ms-surface)] border border-[var(--ms-border)] text-sm hover:border-[var(--ms-turquoise)] transition-colors"
            >
              {testing ? "Testing…" : micOk ? "Mic working ✓" : "Test Microphone"}
            </button>
            {micError && <p className="text-sm text-[var(--ms-error)]">{micError}</p>}
            {micOk && (
              <p className="text-sm text-[var(--ms-turquoise)]">
                Hearing you{micLabel ? ` on ${micLabel}` : ""}.
              </p>
            )}
            <div className="flex justify-end">
              <button onClick={() => setStep("hotkey")} className="btn-primary px-4 py-2 text-sm">
                Next
              </button>
            </div>
          </div>
        )}

        {step === "hotkey" && (
          <div className="space-y-6">
            <h2 className="text-2xl font-semibold">Your Hotkey</h2>
            <p className="text-sm text-[var(--ms-text-dim)]">
              Hold{" "}
              <kbd className="px-2 py-0.5 rounded bg-[var(--ms-surface)] border border-[var(--ms-border)] text-xs text-[var(--ms-turquoise)]">
                {hotkeyLabel}
              </kbd>{" "}
              to dictate. Release to insert. Watch the bar at the bottom of your screen.
            </p>
            <div className="flex justify-end">
              <button onClick={() => setStep("done")} className="btn-primary px-4 py-2 text-sm">
                Next
              </button>
            </div>
          </div>
        )}

        {step === "done" && (
          <div className="text-center space-y-6">
            <div className="text-3xl font-bold accent-gradient-text">You&apos;re ready</div>
            <p className="text-[var(--ms-text-dim)] text-sm">
              MaxSpeech lives in the tray. Hold your hotkey anywhere to start dictating.
            </p>
            <button onClick={() => void finish()} className="btn-orange px-6 py-3 text-sm">
              Start Dictating
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
