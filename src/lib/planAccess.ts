import type { PlanTier } from "./plan";

/** Owner account: Free / Starter / Pro selectable without payment. Max is never free. */
export const OWNER_FREE_PLAN_EMAIL = "pauldimov5@gmail.com";

export function isOwnerFreePlanEmail(email: string | null | undefined): boolean {
  return (email ?? "").trim().toLowerCase() === OWNER_FREE_PLAN_EMAIL;
}

/** Whether this signed-in user may switch to `tier` without checkout. */
export function canSelectTierWithoutPayment(
  email: string | null | undefined,
  tier: PlanTier,
): boolean {
  if (tier === "free") return true;
  if (isOwnerFreePlanEmail(email)) return true;
  return false;
}
