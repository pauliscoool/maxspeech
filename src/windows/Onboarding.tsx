import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Step = "welcome" | "mic" | "apikey" | "hotkey" | "done";

export default function Onboarding() {
  const [step, setStep] = useState<Step>("welcome");
  const [micOk, setMicOk] = useState(false);
  const [deepgramKey, setDeepgramKey] = useState("");
  const [testing, setTesting] = useState(false);

  async function testMic() {
    setTesting(true);
    try {
      await invoke("test_microphone");
      setMicOk(true);
    } catch {
      setMicOk(false);
    } finally {
      setTesting(false);
    }
  }

  async function saveKey() {
    await invoke("save_secret", { key: "deepgram_api_key", value: deepgramKey });
    setStep("hotkey");
  }

  async function finish() {
    await invoke("complete_onboarding");
  }

  return (
    <div className="flex items-center justify-center h-screen bg-[var(--ms-bg)]">
      <div className="w-full max-w-md p-8">
        {step === "welcome" && (
          <div className="text-center space-y-6">
            <div className="text-5xl font-bold bg-gradient-to-r from-indigo-400 to-purple-400 bg-clip-text text-transparent">
              MaxSpeech
            </div>
            <p className="text-[var(--ms-text-dim)]">
              AI-powered voice dictation that types anywhere.
            </p>
            <button
              onClick={() => setStep("mic")}
              className="px-6 py-3 rounded-xl bg-indigo-500 text-white font-medium hover:bg-indigo-600 transition-colors"
            >
              Get Started
            </button>
          </div>
        )}
        {step === "mic" && (
          <div className="space-y-6">
            <h2 className="text-xl font-semibold">Microphone Check</h2>
            <p className="text-sm text-[var(--ms-text-dim)]">
              MaxSpeech needs access to your microphone to transcribe speech.
            </p>
            <button
              onClick={testMic}
              disabled={testing}
              className="px-4 py-2 rounded-lg bg-[var(--ms-surface)] border border-[var(--ms-border)] text-sm hover:border-indigo-500 transition-colors"
            >
              {testing ? "Testing..." : micOk ? "Mic working!" : "Test Microphone"}
            </button>
            <div className="flex justify-end">
              <button
                onClick={() => setStep("apikey")}
                className="px-4 py-2 rounded-lg bg-indigo-500 text-white text-sm font-medium hover:bg-indigo-600"
              >
                Next
              </button>
            </div>
          </div>
        )}
        {step === "apikey" && (
          <div className="space-y-6">
            <h2 className="text-xl font-semibold">Deepgram API Key</h2>
            <p className="text-sm text-[var(--ms-text-dim)]">
              Get a free key at{" "}
              <a href="#" className="text-indigo-400 underline">
                deepgram.com
              </a>
            </p>
            <input
              type="password"
              value={deepgramKey}
              onChange={(e) => setDeepgramKey(e.target.value)}
              placeholder="Paste your Deepgram API key"
              className="w-full px-3 py-2 rounded-lg bg-[var(--ms-surface)] border border-[var(--ms-border)] text-sm focus:outline-none focus:border-indigo-500"
            />
            <div className="flex justify-end">
              <button
                onClick={saveKey}
                disabled={!deepgramKey}
                className="px-4 py-2 rounded-lg bg-indigo-500 text-white text-sm font-medium hover:bg-indigo-600 disabled:opacity-50"
              >
                Save &amp; Continue
              </button>
            </div>
          </div>
        )}
        {step === "hotkey" && (
          <div className="space-y-6">
            <h2 className="text-xl font-semibold">Your Hotkey</h2>
            <p className="text-sm text-[var(--ms-text-dim)]">
              Hold <kbd className="px-2 py-0.5 rounded bg-[var(--ms-border)] text-xs">Ctrl + Space</kbd>{" "}
              to dictate. Release to insert text.
            </p>
            <div className="flex justify-end">
              <button
                onClick={() => setStep("done")}
                className="px-4 py-2 rounded-lg bg-indigo-500 text-white text-sm font-medium hover:bg-indigo-600"
              >
                Next
              </button>
            </div>
          </div>
        )}
        {step === "done" && (
          <div className="text-center space-y-6">
            <div className="text-3xl font-bold">You&apos;re all set!</div>
            <p className="text-[var(--ms-text-dim)] text-sm">
              MaxSpeech is ready. It will run in the system tray.
            </p>
            <button
              onClick={finish}
              className="px-6 py-3 rounded-xl bg-indigo-500 text-white font-medium hover:bg-indigo-600 transition-colors"
            >
              Start Dictating
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
