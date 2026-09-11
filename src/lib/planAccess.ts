import type { PlanTier } from "./plan";

/** Owner account: all plans selectable without payment, plus usage admin. */
export const OWNER_FREE_PLAN_EMAIL = "pauldimov5@gmail.com";

export function isOwnerAccount(email: string | null | undefined): boolean {
  return (email ?? "").trim().toLowerCase() === OWNER_FREE_PLAN_EMAIL;
}

/** @deprecated Use isOwnerAccount */
export function isOwnerFreePlanEmail(email: string | null | undefined): boolean {
  return isOwnerAccount(email);
}

/** Whether this signed-in user may switch to `tier` without checkout. */
export function canSelectTierWithoutPayment(
  email: string | null | undefined,
  tier: PlanTier,
): boolean {
  if (tier === "free") return true;
  if (isOwnerAccount(email)) return true;
  return false;
}
