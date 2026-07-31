import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PageId } from "../Shell";
import { friendlyAppName } from "../../lib/appNames";
import ConfirmModal from "../../components/ConfirmModal";

interface HistoryEntry {
  id: number;
  text: string;
  app_name: string;
  created_at: string;
}

type BtnState = "idle" | "loading" | "ok";

export default function HomePage({
  displayName,
  onNavigate,
  onChanged,
  selectMode = false,
  onSelectModeChange,
}: {
  displayName?: string | null;
  onNavigate: (p: PageId) => void;
  onChanged: () => void;
  selectMode?: boolean;
  onSelectModeChange?: (on: boolean) => void;
}) {
  const [name, setName] = useState(displayName || "there");
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [search, setSearch] = useState("");
  const [copyState, setCopyState] = useState<Record<number, BtnState>>({});
  const [remakeState, setRemakeState] = useState<Record<number, BtnState>>({});
  const [deleteId, setDeleteId] = useState<number | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [bulkConfirm, setBulkConfirm] = useState(false);
  const [bulkBusy, setBulkBusy] = useState(false);

  useEffect(() => {
    if (displayName) {
      setName(displayName);
      return;
    }
    invoke<string>("get_user_name").then(setName).catch(() => {});
  }, [displayName]);

  useEffect(() => {
    const t = setTimeout(() => {
      invoke<HistoryEntry[]>("get_history", { search })
        .then(setEntries)
        .catch(() => setEntries([]));
    }, 150);
    return () => clearTimeout(t);
  }, [search]);

  useEffect(() => {
    if (!selectMode) {
      setSelected(new Set());
      setBulkConfirm(false);
    }
  }, [selectMode]);

  const grouped = useMemo(() => groupByDate(entries), [entries]);

  async function copyText(id: number, text: string) {
    if (copyState[id] === "loading") return;
    setCopyState((s) => ({ ...s, [id]: "loading" }));
    const started = Date.now();
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // ignore
    }
    const wait = Math.max(0, 450 - (Date.now() - started));
    await new Promise((r) => setTimeout(r, wait));
    setCopyState((s) => ({ ...s, [id]: "ok" }));
    setTimeout(() => {
      setCopyState((s) => ({ ...s, [id]: "idle" }));
    }, 700);
  }

  async function confirmDelete() {
    if (deleteId == null) return;
    setDeleting(true);
    try {
      await invoke("delete_history", { id: deleteId });
      setEntries((prev) => prev.filter((e) => e.id !== deleteId));
      onChanged();
      setDeleteId(null);
    } catch (e) {
      console.error(e);
    } finally {
      setDeleting(false);
    }
  }

  async function remake(id: number) {
    if (remakeState[id] === "loading") return;
    setRemakeState((s) => ({ ...s, [id]: "loading" }));
    try {
      await invoke("remake_dictation", { id });
      setRemakeState((s) => ({ ...s, [id]: "ok" }));
      setTimeout(() => {
        setRemakeState((s) => ({ ...s, [id]: "idle" }));
      }, 600);
    } catch (e) {
      console.error(e);
      setRemakeState((s) => ({ ...s, [id]: "idle" }));
    }
  }

  function toggleSelect(id: number) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function confirmBulkDelete() {
    if (selected.size === 0) return;
    setBulkBusy(true);
    try {
      const ids = Array.from(selected);
      await invoke("delete_history_many", { ids });
      setEntries((prev) => prev.filter((e) => !selected.has(e.id)));
      setSelected(new Set());
      setBulkConfirm(false);
      onSelectModeChange?.(false);
      onChanged();
    } catch (e) {
      console.error(e);
    } finally {
      setBulkBusy(false);
    }
  }

  return (
    <div className="page-shell space-y-5">
      <h1 className="page-title">
        Welcome back, <span className="accent-gradient-text">{name}</span>
      </h1>

      <div className="relative overflow-hidden rounded-2xl bg-[var(--ms-surface)]">
        <div
          className="absolute inset-0 opacity-40 pointer-events-none"
          style={{
            background:
              "radial-gradient(ellipse at 20% 50%, rgba(45,212,191,0.35), transparent 55%), radial-gradient(ellipse at 90% 20%, rgba(249,115,22,0.28), transparent 50%)",
          }}
        />
        <div className="relative p-4 sm:p-5 space-y-3">
          <p className="text-sm text-[var(--ms-text-soft)] leading-relaxed">
            Dictate anywhere. Use your hotkey, speak, release — MaxSpeech types into whatever
            app you&apos;re in.
          </p>
          <div className="flex flex-wrap items-center gap-2.5">
            <button
              onClick={() => onNavigate("transforms")}
              className="btn-primary px-4 py-2 text-xs"
            >
              Try transforms
            </button>
            <button
              onClick={() => onNavigate("settings")}
              className="text-xs text-[var(--ms-text-dim)] hover:text-[var(--ms-turquoise)] transition-colors px-1"
            >
              Set hotkey
            </button>
          </div>
        </div>
      </div>

      <div className="flex items-center gap-3">
        <span className="text-[11px] tracking-wider text-[var(--ms-text-dim)] uppercase shrink-0">
          {selectMode ? "Select to delete" : "Activity"}
        </span>
        <div className="h-px flex-1" style={{ background: "var(--ms-hairline)" }} />
        {selectMode ? (
          <div className="flex items-center gap-2 shrink-0">
            <button
              type="button"
              onClick={() => onSelectModeChange?.(false)}
              className="text-[11px] px-2.5 py-1 rounded-full text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)] transition-colors"
              style={{ background: "var(--ms-fill-muted)" }}
            >
              Cancel
            </button>
            <button
              type="button"
              disabled={selected.size === 0}
              onClick={() => setBulkConfirm(true)}
              className="text-[11px] px-2.5 py-1 rounded-full font-semibold bg-[var(--ms-error)] text-white disabled:opacity-40 transition-opacity"
            >
              Delete ({selected.size})
            </button>
          </div>
        ) : (
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search…"
            className="input-field px-3 py-1.5 text-xs w-36 sm:w-44 shrink-0"
          />
        )}
      </div>

      {entries.length === 0 ? (
        <div className="surface-card p-8 text-center space-y-2">
          <p className="text-sm text-[var(--ms-text)]">No dictations yet</p>
          <p className="text-xs text-[var(--ms-text-dim)]">
            Hold your hotkey and say something — it&apos;ll show up here.
          </p>
        </div>
      ) : (
        <div className="space-y-5">
          {Object.entries(grouped).map(([date, items]) => (
            <section key={date} className="space-y-2.5">
              <div className="flex items-center gap-3">
                <span className="text-[11px] tracking-wider text-[var(--ms-text-dim)] uppercase shrink-0">
                  {date}
                </span>
                <div className="h-px flex-1" style={{ background: "var(--ms-hairline)" }} />
              </div>
              {items.map((entry) => {
                const cState = copyState[entry.id] ?? "idle";
                const rState = remakeState[entry.id] ?? "idle";
                const isSelected = selected.has(entry.id);
                return (
                  <article
                    key={entry.id}
                    className={`surface-card p-3.5 sm:p-4 group ${
                      selectMode && isSelected
                        ? "ring-1 ring-[var(--ms-error)]/50 bg-[var(--ms-error)]/5"
                        : ""
                    }`}
                  >
                    <div className="flex flex-col gap-2.5">
                      <div className="flex items-start justify-between gap-2">
                        <div className="flex items-start gap-2.5 min-w-0">
                          {selectMode && (
                            <input
                              type="checkbox"
                              checked={isSelected}
                              onChange={() => toggleSelect(entry.id)}
                              className="mt-0.5 accent-[var(--ms-error)] shrink-0"
                              aria-label={`Select dictation from ${formatTime(entry.created_at)}`}
                            />
                          )}
                          <div className="text-[11px] text-[var(--ms-text-dim)] min-w-0">
                            {formatTime(entry.created_at)}
                            {entry.app_name ? (
                              <>
                                {" · "}
                                <span className="text-[var(--ms-turquoise)]">
                                  {friendlyAppName(entry.app_name)}
                                </span>
                              </>
                            ) : null}
                          </div>
                        </div>
                        {!selectMode && (
                          <div
                            className={`flex gap-1 shrink-0 transition-opacity ${
                              cState !== "idle" || rState !== "idle"
                                ? "opacity-100"
                                : "opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto"
                            }`}
                          >
                            <IconBtn
                              label={cState === "ok" ? "Copied" : "Copy"}
                              state={cState}
                              onClick={() => copyText(entry.id, entry.text)}
                            />
                            <IconBtn
                              label={rState === "loading" ? "…" : rState === "ok" ? "Done" : "Remake"}
                              state={rState}
                              onClick={() => remake(entry.id)}
                              title="Re-enhance & type into the focused app"
                            />
                            <IconBtn
                              label="Delete"
                              onClick={() => setDeleteId(entry.id)}
                              danger
                            />
                          </div>
                        )}
                      </div>
                      <p
                        className="text-sm text-[var(--ms-text-soft)] leading-relaxed whitespace-pre-wrap break-words"
                        onClick={selectMode ? () => toggleSelect(entry.id) : undefined}
                        style={selectMode ? { cursor: "pointer" } : undefined}
                      >
                        {entry.text}
                      </p>
                    </div>
                  </article>
                );
              })}
            </section>
          ))}
        </div>
      )}

      <ConfirmModal
        open={deleteId != null}
        title="Delete this dictation?"
        description="This removes it from your local history. You can’t undo this."
        confirmLabel="Delete"
        destructive
        busy={deleting}
        onCancel={() => {
          if (!deleting) setDeleteId(null);
        }}
        onConfirm={confirmDelete}
      />

      <ConfirmModal
        open={bulkConfirm}
        title={`Delete ${selected.size} dictation${selected.size === 1 ? "" : "s"}?`}
        description="Selected items will be removed from local history permanently."
        confirmLabel="Delete selected"
        destructive
        busy={bulkBusy}
        onCancel={() => {
          if (!bulkBusy) setBulkConfirm(false);
        }}
        onConfirm={confirmBulkDelete}
      />
    </div>
  );
}

function IconBtn({
  label,
  onClick,
  danger,
  title,
  state = "idle",
}: {
  label: string;
  onClick: () => void;
  danger?: boolean;
  title?: string;
  state?: BtnState;
}) {
  const busy = state === "loading";
  const ok = state === "ok";
  return (
    <button
      onClick={onClick}
      disabled={busy}
      title={title || label}
      className={`text-[11px] px-2 py-1 rounded-full transition-all duration-200 inline-flex items-center gap-1.5 ${
        busy ? "ms-btn-busy" : ""
      } ${ok ? "ms-btn-ok" : ""} ${
        danger && !ok
          ? "hover:bg-[var(--ms-error)]/20 hover:text-[var(--ms-error)]"
          : !ok
            ? "hover:bg-[var(--ms-turquoise-glow)] hover:text-[var(--ms-turquoise)]"
            : ""
      }`}
      style={ok ? undefined : { background: "var(--ms-fill-muted)" }}
    >
      {busy ? <span className="ms-spin" aria-hidden /> : null}
      {label}
    </button>
  );
}

function groupByDate(entries: HistoryEntry[]) {
  const map: Record<string, HistoryEntry[]> = {};
  for (const e of entries) {
    const d = new Date(e.created_at.includes("T") ? e.created_at : e.created_at + "Z");
    const key = isNaN(d.getTime())
      ? e.created_at.slice(0, 10)
      : d
          .toLocaleDateString(undefined, {
            month: "short",
            day: "numeric",
            year: "numeric",
          })
          .toUpperCase();
    (map[key] ||= []).push(e);
  }
  return map;
}

function formatTime(iso: string) {
  const d = new Date(iso.includes("T") ? iso : iso + "Z");
  if (isNaN(d.getTime())) return iso;
  return d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
}
