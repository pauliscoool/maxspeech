import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import {
  PLAN_OPTIONS,
  formatWeeklyUsage,
  planLabel,
  type PlanStatus,
  type PlanTier,
} from "../lib/plan";
import {
  canSelectTierWithoutPayment,
  isOwnerFreePlanEmail,
} from "../lib/planAccess";
import { updateCloudPlan, type AuthUser } from "../lib/auth";
import {
  pushCloudSettings,
  syncAllLocalHistoryIfMax,
} from "../lib/cloudSync";

export default function PlanModal({
  open,
  plan,
  authUser,
  onClose,
  onChanged,
}: {
  open: boolean;
  plan: PlanStatus | null;
  authUser: AuthUser | null;
  onClose: () => void;
  onChanged: () => void;
}) {
  const [localPlan, setLocalPlan] = useState<PlanStatus | null>(plan);
  const [settingTier, setSettingTier] = useState<PlanTier | null>(null);
  const [planMsg, setPlanMsg] = useState("");

  useEffect(() => {
    if (open) {
      setLocalPlan(plan);
      setPlanMsg("");
      setSettingTier(null);
    }
  }, [open, plan]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !settingTier) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, settingTier, onClose]);

  async function selectTier(tier: PlanTier) {
    if (settingTier || localPlan?.tier === tier) return;
    if (!canSelectTierWithoutPayment(authUser?.email, tier)) {
      setPlanMsg(
        tier === "max"
          ? "Max isn't available as a free plan — payment checkout is coming soon."
          : "Payment checkout coming soon for paid plans.",
      );
      return;
    }
    setSettingTier(tier);
    setPlanMsg("");
    try {
      await invoke("set_plan_tier", { tier });
      await updateCloudPlan(tier);
      await pushCloudSettings();
      if (tier === "max") await syncAllLocalHistoryIfMax();
      const next = await invoke<PlanStatus>("get_plan_status");
      setLocalPlan(next);
      setPlanMsg(
        tier === "max"
          ? "Switched to Max — dictation history will sync to the cloud."
          : `Switched to ${PLAN_OPTIONS.find((p) => p.tier === tier)?.label ?? tier}.`,
      );
      onChanged();
    } catch (e) {
      setPlanMsg(`Could not set plan: ${e}`);
    } finally {
      setSettingTier(null);
    }
  }

  if (!open) return null;

  return createPortal(
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center p-4 ms-modal-backdrop"
      onClick={() => {
        if (!settingTier) onClose();
      }}
      role="presentation"
    >
      <div
        className="ms-modal surface-card w-full max-w-md p-5 space-y-4"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="ms-plan-title"
      >
        <div className="space-y-1">
          <h2 id="ms-plan-title" className="text-base font-semibold text-[var(--ms-text)]">
            Choose a plan
          </h2>
          <p className="text-sm text-[var(--ms-text-dim)] leading-relaxed">
            {localPlan
              ? `Current: ${planLabel(localPlan.tier)}${
                  localPlan.weekly_limit != null
                    ? ` · ${formatWeeklyUsage(localPlan)} words this week`
                    : ""
                }`
              : "Pick the weekly word allowance that fits how you dictate."}
          </p>
        </div>

        <div className="grid grid-cols-2 gap-2">
          {PLAN_OPTIONS.map((opt) => {
            const active = localPlan?.tier === opt.tier;
            const busy = settingTier === opt.tier;
            const allowed = canSelectTierWithoutPayment(authUser?.email, opt.tier);
            const locked = !active && !allowed;
            return (
              <button
                key={opt.tier}
                type="button"
                onClick={() => void selectTier(opt.tier)}
                disabled={!!settingTier || active || locked}
                className={`text-left p-3 rounded-2xl transition-all disabled:cursor-default ${
                  active
                    ? "bg-[var(--ms-turquoise-glow)] ring-1 ring-[var(--ms-turquoise)]/40"
                    : locked
                      ? "opacity-55 text-[var(--ms-text-dim)]"
                      : "text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)]"
                }`}
                style={active ? undefined : { background: "var(--ms-fill-muted)" }}
              >
                <div className="flex items-baseline justify-between gap-2">
                  <div
                    className={`text-sm font-medium ${
                      active ? "text-[var(--ms-turquoise)]" : ""
                    }`}
                  >
                    {opt.label}
                  </div>
                  <div
                    className={`text-xs font-semibold ${
                      active ? "text-[var(--ms-turquoise)]" : "text-[var(--ms-orange)]"
                    }`}
                  >
                    {opt.price}
                    {opt.tier !== "free" ? <span className="opacity-60">/mo</span> : null}
                  </div>
                </div>
                <div className="text-[11px] mt-1 opacity-80 leading-snug">{opt.limit}</div>
                <div
                  className={`text-[10px] mt-2 font-medium ${
                    active ? "text-[var(--ms-turquoise)]" : "opacity-60"
                  }`}
                >
                  {busy
                    ? "Switching…"
                    : active
                      ? "Current plan"
                      : locked
                        ? opt.tier === "max"
                          ? "Payment soon"
                          : "Locked"
                        : "Select"}
                </div>
              </button>
            );
          })}
        </div>

        <p className="text-[11px] text-[var(--ms-text-dim)] leading-relaxed">
          {isOwnerFreePlanEmail(authUser?.email)
            ? "Owner access: all plans including Max are selectable on this account — your devices sync settings, dictionary, snippets, and style automatically. Cloud history sync included with Max."
            : "Free plan is available now. Paid plans unlock when checkout ships. Cloud history sync is included with Max."}
        </p>
        {planMsg ? (
          <p className="text-xs text-[var(--ms-turquoise)]">{planMsg}</p>
        ) : null}

        <div className="flex justify-end pt-1">
          <button
            type="button"
            onClick={onClose}
            disabled={!!settingTier}
            className="px-3.5 py-1.5 text-xs rounded-full text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)] transition-colors disabled:opacity-50"
            style={{ background: "var(--ms-fill-muted)" }}
          >
            Done
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
