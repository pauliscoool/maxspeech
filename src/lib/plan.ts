export type PlanTier = "free" | "starter" | "pro" | "max";

export type PlanStatus = {
  tier: PlanTier;
  price_usd: number;
  weekly_limit: number | null;
  words_used: number;
  words_remaining: number | null;
  week_starts_at: string;
  can_dictate: boolean;
};

export const PLAN_OPTIONS: {
  tier: PlanTier;
  label: string;
  price: string;
  limit: string;
}[] = [
  { tier: "free", label: "Free", price: "$0", limit: "1,500 words / week" },
  { tier: "starter", label: "Starter", price: "$3", limit: "4,500 words / week" },
  { tier: "pro", label: "Pro", price: "$5", limit: "10,000 words / week" },
  { tier: "max", label: "Max", price: "$10", limit: "25,000 words / week" },
];

export function planLabel(tier: PlanTier): string {
  return PLAN_OPTIONS.find((o) => o.tier === tier)?.label ?? "Free";
}

export function formatWeeklyUsage(status: PlanStatus): string {
  if (status.weekly_limit == null) return "—";
  return `${status.words_used.toLocaleString()} / ${status.weekly_limit.toLocaleString()}`;
}

export function weeklyUsagePct(status: PlanStatus): number | null {
  if (status.weekly_limit == null || status.weekly_limit <= 0) return null;
  return Math.min(100, Math.round((status.words_used / status.weekly_limit) * 100));
}
