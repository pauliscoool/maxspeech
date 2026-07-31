import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatWeeklyUsage, weeklyUsagePct, planLabel, type PlanStatus } from "../../lib/plan";

interface Stats {
  total_words: number;
  total_entries: number;
  days_active: number;
  avg_wpm: number;
}

export default function InsightsPage() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [plan, setPlan] = useState<PlanStatus | null>(null);

  useEffect(() => {
    invoke<Stats>("get_stats").then(setStats).catch(() => {});
    invoke<PlanStatus>("get_plan_status")
      .then(setPlan)
      .catch(() => setPlan(null));
  }, []);

  const showMeter = plan?.weekly_limit != null;
  const pct = plan ? weeklyUsagePct(plan) : null;

  return (
    <div className="page-shell space-y-5">
      <header>
        <h1 className="page-title">Insights</h1>
        <p className="page-subtitle">Your dictation rhythm at a glance.</p>
      </header>

      {showMeter && plan && (
        <div className="surface-card p-4 sm:p-5 space-y-3">
          <div className="flex items-baseline justify-between gap-3">
            <div>
              <div className="text-sm font-medium">{planLabel(plan.tier)} weekly usage</div>
              <div className="text-[11px] text-[var(--ms-text-dim)] mt-0.5">
                {!plan.can_dictate
                  ? plan.tier === "max"
                    ? "Limit reached — resets Monday (UTC)"
                    : "Limit reached — upgrade in Settings"
                  : "Resets every Monday (UTC)"}
              </div>
            </div>
            <div
              className={`text-xl font-semibold tabular-nums tracking-tight ${
                !plan.can_dictate
                  ? "text-[var(--ms-orange)]"
                  : "text-[var(--ms-turquoise)]"
              }`}
            >
              {formatWeeklyUsage(plan)}
            </div>
          </div>
          <div
            className="h-1.5 rounded-full overflow-hidden"
            style={{ background: "var(--ms-fill-track)" }}
          >
            <div
              className={`h-full rounded-full transition-all ${
                !plan.can_dictate
                  ? "bg-[var(--ms-orange)]"
                  : "bg-[var(--ms-turquoise)]"
              }`}
              style={{ width: `${pct ?? 0}%` }}
            />
          </div>
        </div>
      )}

      <div className="grid grid-cols-2 gap-3">
        <InsightCard
          title="Words dictated"
          value={stats ? format(stats.total_words) : "—"}
          hint="All time"
          accent="turquoise"
        />
        <InsightCard
          title="Dictations"
          value={stats ? String(stats.total_entries) : "—"}
          hint="Sessions saved"
          accent="orange"
        />
        <InsightCard
          title="Active days"
          value={stats ? String(stats.days_active) : "—"}
          hint="Days with activity"
          accent="turquoise"
        />
        <InsightCard
          title="Est. WPM"
          value={stats && stats.avg_wpm > 0 ? String(stats.avg_wpm) : "—"}
          hint="Speaking pace"
          accent="orange"
        />
      </div>

      <div className="surface-card p-4 sm:p-5">
        <h2 className="text-sm font-medium mb-2">Tip</h2>
        <p className="text-sm text-[var(--ms-text-dim)] leading-relaxed">
          Voice commands like &quot;scratch that&quot; or &quot;make it formal&quot; edit what you
          just typed. AI rewrite commands use the app&apos;s built-in enhance path.
        </p>
      </div>
    </div>
  );
}

function InsightCard({
  title,
  value,
  hint,
  accent,
}: {
  title: string;
  value: string;
  hint: string;
  accent: "turquoise" | "orange";
}) {
  return (
    <div className="surface-card p-4">
      <div className="text-xs text-[var(--ms-text-dim)] mb-1.5">{title}</div>
      <div
        className={`text-2xl sm:text-3xl font-semibold tracking-tight ${
          accent === "orange" ? "text-[var(--ms-orange)]" : "text-[var(--ms-turquoise)]"
        }`}
      >
        {value}
      </div>
      <div className="text-[11px] text-[var(--ms-text-dim)] mt-1.5">{hint}</div>
    </div>
  );
}

function format(n: number) {
  if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
  return String(n);
}
