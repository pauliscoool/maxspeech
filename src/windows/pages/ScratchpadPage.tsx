import { useState } from "react";

export default function ScratchpadPage() {
  const [text, setText] = useState("");

  return (
    <div className="page-shell h-full flex flex-col gap-4 !max-w-none">
      <header>
        <h1 className="page-title">Scratchpad</h1>
        <p className="page-subtitle">
          A quiet place to dump thoughts. Dictate into this window with your hotkey.
        </p>
      </header>
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="Start typing or dictating…"
        className="input-field flex-1 min-h-[280px] w-full p-4 text-sm leading-relaxed resize-none"
      />
      <div className="flex justify-between text-xs text-[var(--ms-text-dim)]">
        <span>{text.split(/\s+/).filter(Boolean).length} words</span>
        <button
          onClick={() => navigator.clipboard.writeText(text)}
          disabled={!text}
          className="hover:text-[var(--ms-turquoise)] transition-colors disabled:opacity-40"
        >
          Copy all
        </button>
      </div>
    </div>
  );
}
