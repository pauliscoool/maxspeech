import type { AuthUser } from "../../lib/auth";
import type { PlanStatus } from "../../lib/plan";
import UsageTab from "./UsageTab";

export default function UsagePage({
  authUser,
  plan,
  onPlanChanged,
}: {
  authUser?: AuthUser | null;
  plan: PlanStatus | null;
  onPlanChanged: () => void;
}) {
  return (
    <div className="page-shell space-y-7">
      <header>
        <h1 className="page-title">Usage</h1>
        <p className="page-subtitle">
          Plan allowance and what you&apos;ve used. Paid plans reset Monday (UTC).
        </p>
      </header>
      <UsageTab authUser={authUser} plan={plan} onPlanChanged={onPlanChanged} />
    </div>
  );
}
