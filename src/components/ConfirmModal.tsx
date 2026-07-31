import { useEffect } from "react";

export default function ConfirmModal({
  open,
  title,
  description,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  destructive = false,
  busy = false,
  onConfirm,
  onCancel,
}: {
  open: boolean;
  title: string;
  description?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  destructive?: boolean;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, busy, onCancel]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center p-4 ms-modal-backdrop"
      onClick={() => {
        if (!busy) onCancel();
      }}
      role="presentation"
    >
      <div
        className="ms-modal surface-card w-full max-w-sm p-5 space-y-4"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="ms-confirm-title"
      >
        <div className="space-y-1.5">
          <h2 id="ms-confirm-title" className="text-base font-semibold text-[var(--ms-text)]">
            {title}
          </h2>
          {description ? (
            <p className="text-sm text-[var(--ms-text-dim)] leading-relaxed">{description}</p>
          ) : null}
        </div>
        <div className="flex justify-end gap-2 pt-1">
          <button
            type="button"
            onClick={onCancel}
            disabled={busy}
            className="px-3.5 py-1.5 text-xs rounded-full text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)] transition-colors disabled:opacity-50"
            style={{ background: "var(--ms-fill-muted)" }}
          >
            {cancelLabel}
          </button>
          <button
            type="button"
            onClick={onConfirm}
            disabled={busy}
            className={`px-3.5 py-1.5 text-xs rounded-full font-semibold transition-all disabled:opacity-50 ${
              destructive
                ? "bg-[var(--ms-error)] text-white hover:brightness-110"
                : "btn-primary"
            }`}
          >
            {busy ? "Working…" : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
