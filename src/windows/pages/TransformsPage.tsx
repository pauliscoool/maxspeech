export default function TransformsPage() {
  const transforms = [
    { say: "make it formal", does: "Rewrites the last dictation in a professional tone" },
    { say: "make it shorter", does: "Condenses the last insertion" },
    { say: "make it casual", does: "Loosens tone for chat apps" },
    { say: "scratch that", does: "Deletes the last insertion" },
    { say: "new line", does: "Inserts a line break" },
    { say: "bullet that", does: "Coming soon — formats as a list" },
  ];

  return (
    <div className="page-shell space-y-5">
      <header>
        <h1 className="page-title">Transforms</h1>
        <p className="page-subtitle">
          Speak a command after dictating — MaxSpeech rewrites or edits what it just typed.
        </p>
      </header>

      <div className="relative overflow-hidden rounded-2xl bg-[var(--ms-surface)] p-4 sm:p-5">
        <div
          className="absolute inset-0 opacity-30 pointer-events-none"
          style={{
            background:
              "radial-gradient(circle at 80% 30%, rgba(249,115,22,0.35), transparent 50%), radial-gradient(circle at 10% 80%, rgba(45,212,191,0.3), transparent 45%)",
          }}
        />
        <p className="relative text-sm text-[var(--ms-text-soft)] leading-relaxed">
          Transforms work anywhere you write. Dictate first, then say a command to clean up,
          rewrite, or undo.
        </p>
      </div>

      <div className="space-y-2">
        {transforms.map((t) => (
          <div
            key={t.say}
            className="surface-card p-3.5 sm:p-4 flex flex-col sm:flex-row sm:items-start gap-2 sm:gap-4"
          >
            <code
              className="text-xs px-2 py-1 rounded-lg text-[var(--ms-turquoise)] shrink-0 w-fit"
              style={{ background: "var(--ms-kbd-bg)" }}
            >
              &quot;{t.say}&quot;
            </code>
            <p className="text-sm text-[var(--ms-text-dim)] leading-snug">{t.does}</p>
          </div>
        ))}
      </div>
    </div>
  );
}
