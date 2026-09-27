import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  PLAN_OPTIONS,
  formatWeeklyUsage,
  planLabel,
  usageMeterTitle,
  usageResetHint,
  weeklyUsagePct,
  type PlanStatus,
  type PlanTier,
} from "../../lib/plan";
import { canSelectTierWithoutPayment, isOwnerAccount } from "../../lib/planAccess";
import { updateCloudPlan, type AuthUser } from "../../lib/auth";
import {
  pushCloudSettings,
  syncAllLocalHistoryIfMax,
} from "../../lib/cloudSync";
import {
  getCloudPerson,
  listCloudPeople,
  setCloudPersonBonus,
  setCloudPersonPlan,
  setCloudPersonWordsUsed,
  type CloudPerson,
} from "../../lib/ownerAdmin";

export default function UsageTab({
  authUser,
  plan,
  onPlanChanged,
}: {
  authUser?: AuthUser | null;
  plan: PlanStatus | null;
  onPlanChanged: () => void;
}) {
  const owner = isOwnerAccount(authUser?.email);
  const [msg, setMsg] = useState("");
  const [busy, setBusy] = useState(false);
  const [grant, setGrant] = useState("1000");
  const [usedEdit, setUsedEdit] = useState("");
  const [people, setPeople] = useState<CloudPerson[]>([]);
  const [peopleErr, setPeopleErr] = useState("");
  const [peopleBusy, setPeopleBusy] = useState<string | null>(null);
  const [uuidQuery, setUuidQuery] = useState("");
  const [lookup, setLookup] = useState<CloudPerson | null>(null);
  const [lookupUsed, setLookupUsed] = useState("");
  const [lookupGrant, setLookupGrant] = useState("1000");
  const [lookupBusy, setLookupBusy] = useState(false);

  useEffect(() => {
    if (plan) setUsedEdit(String(plan.words_used));
  }, [plan?.words_used]);

  useEffect(() => {
    if (!owner) return;
    let cancelled = false;
    void listCloudPeople(authUser?.email)
      .then((rows) => {
        if (!cancelled) {
          setPeople(rows);
          setPeopleErr("");
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setPeopleErr(
            e instanceof Error
              ? e.message
              : "Could not load accounts. Apply supabase/owner-admin.sql if this is only your row.",
          );
        }
      });
    return () => {
      cancelled = true;
    };
  }, [owner, authUser?.email, plan?.tier, plan?.bonus_words]);

  async function selectTier(tier: PlanTier) {
    if (busy || plan?.tier === tier) return;
    if (!canSelectTierWithoutPayment(authUser?.email, tier)) {
      setMsg(
        tier === "max"
          ? "Max isn't available as a free plan — payment checkout is coming soon."
          : "Payment checkout coming soon for paid plans.",
      );
      return;
    }
    setBusy(true);
    setMsg("");
    try {
      await invoke("set_plan_tier", { tier });
      await updateCloudPlan(tier);
      await pushCloudSettings();
      if (tier === "max") await syncAllLocalHistoryIfMax();
      window.dispatchEvent(new Event("maxspeech-plan-changed"));
      setMsg(`Plan set to ${planLabel(tier)}.`);
      onPlanChanged();
    } catch (e) {
      setMsg(`Could not set plan: ${e}`);
    } finally {
      setBusy(false);
    }
  }

  async function addBonus() {
    if (!owner || busy) return;
    const extra = Math.max(0, Math.floor(Number(grant) || 0));
    setBusy(true);
    setMsg("");
    try {
      const current = plan?.bonus_words ?? 0;
      await invoke("set_usage_bonus", { bonus: current + extra });
      await pushCloudSettings();
      window.dispatchEvent(new Event("maxspeech-plan-changed"));
      setMsg(
        extra === 1
          ? "Added 1 bonus to this week’s cap."
          : `Added ${extra.toLocaleString()} bonus to this week’s cap.`,
      );
      onPlanChanged();
    } catch (e) {
      setMsg(`Could not add usage: ${e}`);
    } finally {
      setBusy(false);
    }
  }

  async function saveUsed() {
    if (!owner || busy) return;
    const words = Math.max(0, Math.floor(Number(usedEdit) || 0));
    setBusy(true);
    setMsg("");
    try {
      await invoke("set_words_used", { words });
      window.dispatchEvent(new Event("maxspeech-plan-changed"));
      setMsg("This week’s used words updated.");
      onPlanChanged();
    } catch (e) {
      setMsg(`Could not edit usage: ${e}`);
    } finally {
      setBusy(false);
    }
  }

  async function changePersonPlan(person: CloudPerson, tier: PlanTier) {
    setPeopleBusy(person.id);
    setPeopleErr("");
    try {
      await setCloudPersonPlan(authUser?.email, person.id, tier);
      setPeople((rows) =>
        rows.map((r) => (r.id === person.id ? { ...r, plan_tier: tier } : r)),
      );
    } catch (e) {
      setPeopleErr(e instanceof Error ? e.message : String(e));
    } finally {
      setPeopleBusy(null);
    }
  }

  async function addPersonBonus(person: CloudPerson) {
    const extra = Math.max(0, Math.floor(Number(grant) || 0));
    setPeopleBusy(person.id);
    setPeopleErr("");
    try {
      await setCloudPersonBonus(
        authUser?.email,
        person.id,
        person.usage_bonus + extra,
        plan?.week_starts_at || "",
      );
      setPeople((rows) =>
        rows.map((r) =>
          r.id === person.id ? { ...r, usage_bonus: r.usage_bonus + extra } : r,
        ),
      );
    } catch (e) {
      setPeopleErr(e instanceof Error ? e.message : String(e));
    } finally {
      setPeopleBusy(null);
    }
  }

  async function loadByUuid() {
    if (!owner || lookupBusy) return;
    setLookupBusy(true);
    setPeopleErr("");
    try {
      const person = await getCloudPerson(authUser?.email, uuidQuery);
      setLookup(person);
      setLookupUsed(String(person.words_used || 0));
      setLookupGrant("1000");
    } catch (e) {
      setLookup(null);
      setPeopleErr(e instanceof Error ? e.message : String(e));
    } finally {
      setLookupBusy(false);
    }
  }

  async function changeLookupPlan(tier: PlanTier) {
    if (!lookup) return;
    setLookupBusy(true);
    setPeopleErr("");
    try {
      await setCloudPersonPlan(authUser?.email, lookup.id, tier);
      setLookup({ ...lookup, plan_tier: tier });
      setPeople((rows) =>
        rows.map((r) => (r.id === lookup.id ? { ...r, plan_tier: tier } : r)),
      );
    } catch (e) {
      setPeopleErr(e instanceof Error ? e.message : String(e));
    } finally {
      setLookupBusy(false);
    }
  }

  async function addLookupBonus() {
    if (!lookup) return;
    const extra = Math.max(0, Math.floor(Number(lookupGrant) || 0));
    setLookupBusy(true);
    setPeopleErr("");
    try {
      const next = lookup.usage_bonus + extra;
      await setCloudPersonBonus(
        authUser?.email,
        lookup.id,
        next,
        plan?.week_starts_at || "",
      );
      setLookup({ ...lookup, usage_bonus: next });
      setPeople((rows) =>
        rows.map((r) => (r.id === lookup.id ? { ...r, usage_bonus: next } : r)),
      );
    } catch (e) {
      setPeopleErr(e instanceof Error ? e.message : String(e));
    } finally {
      setLookupBusy(false);
    }
  }

  async function saveLookupUsed() {
    if (!lookup) return;
    const words = Math.max(0, Math.floor(Number(lookupUsed) || 0));
    setLookupBusy(true);
    setPeopleErr("");
    try {
      await setCloudPersonWordsUsed(
        authUser?.email,
        lookup.id,
        words,
        plan?.week_starts_at || "",
      );
      setLookup({ ...lookup, words_used: words });
    } catch (e) {
      setPeopleErr(e instanceof Error ? e.message : String(e));
    } finally {
      setLookupBusy(false);
    }
  }

  const pct = plan ? weeklyUsagePct(plan) : null;
  const bonus = plan?.bonus_words ?? 0;

  return (
    <div className="space-y-7">
      <section className="space-y-2.5">
        <h2 className="settings-section-title">
          {plan?.daily_seconds_limit != null ? "Last 24 hours" : "This week"}
        </h2>
        <div className="settings-group p-4 space-y-3">
          {plan ? (
            <>
              <div className="flex items-baseline justify-between gap-3">
                <div>
                  <div className="text-sm font-medium">{usageMeterTitle(plan)}</div>
                  <div className="text-[11px] text-[var(--ms-text-dim)] mt-0.5">
                    {usageResetHint(plan)}
                    {bonus > 0
                      ? plan.daily_seconds_limit != null
                        ? ` · +${bonus.toLocaleString()} bonus seconds`
                        : ` · +${bonus.toLocaleString()} bonus words`
                      : ""}
                  </div>
                </div>
                <div className="text-xl font-semibold tabular-nums text-[var(--ms-turquoise)]">
                  {formatWeeklyUsage(plan)}
                </div>
              </div>
              <div
                className="h-2.5 rounded-full overflow-hidden"
                style={{ background: "var(--ms-fill-track)" }}
              >
                <div
                  className="h-full rounded-full bg-[var(--ms-turquoise)]"
                  style={{ width: `${pct ?? 0}%` }}
                />
              </div>
            </>
          ) : (
            <p className="text-sm text-[var(--ms-text-dim)]">Loading usage…</p>
          )}
        </div>
      </section>

      <section className="space-y-2.5">
        <h2 className="settings-section-title">Plan</h2>
        <div className="settings-group p-3">
          <div className="grid grid-cols-2 gap-2">
            {PLAN_OPTIONS.map((opt) => {
              const active = plan?.tier === opt.tier;
              const allowed = canSelectTierWithoutPayment(authUser?.email, opt.tier);
              const locked = !active && !allowed;
              return (
                <button
                  key={opt.tier}
                  type="button"
                  disabled={busy || active || locked}
                  onClick={() => void selectTier(opt.tier)}
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
                    <div className="text-xs font-semibold text-[var(--ms-orange)]">
                      {opt.price}
                    </div>
                  </div>
                  <div className="text-[11px] mt-1 opacity-80">{opt.limit}</div>
                  <div className="text-[10px] mt-2 font-medium opacity-70">
                    {active ? "Current plan" : locked ? "Locked" : "Select"}
                  </div>
                </button>
              );
            })}
          </div>
        </div>
      </section>

      {owner && (
        <section className="space-y-2.5">
          <h2 className="settings-section-title">Edit this device</h2>
          <div className="settings-group">
            <div className="settings-row">
              <div className="settings-row-text">
                <div className="settings-row-title">Add bonus</div>
                <div className="settings-row-desc">
                  Extra words this UTC week on paid plans, or extra seconds on Free
                </div>
              </div>
              <div className="flex items-center gap-2 shrink-0">
                <input
                  type="number"
                  min={0}
                  step={100}
                  value={grant}
                  onChange={(e) => setGrant(e.target.value)}
                  className="w-24 px-2.5 py-1.5 rounded-xl bg-[var(--ms-bg)] text-sm outline-none"
                />
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void addBonus()}
                  className="btn-primary px-3 py-1.5 text-xs disabled:opacity-50"
                >
                  Add
                </button>
              </div>
            </div>
            <div className="settings-row">
              <div className="settings-row-text">
                <div className="settings-row-title">Words used this week</div>
                <div className="settings-row-desc">
                  Set the billed count on this PC (does not delete history)
                </div>
              </div>
              <div className="flex items-center gap-2 shrink-0">
                <input
                  type="number"
                  min={0}
                  value={usedEdit}
                  onChange={(e) => setUsedEdit(e.target.value)}
                  className="w-24 px-2.5 py-1.5 rounded-xl bg-[var(--ms-bg)] text-sm outline-none"
                />
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void saveUsed()}
                  className="px-3 py-1.5 text-xs rounded-full font-semibold text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)]"
                  style={{ background: "var(--ms-fill-muted)" }}
                >
                  Save
                </button>
              </div>
            </div>
          </div>
        </section>
      )}

      {owner && (
        <section className="space-y-2.5">
          <h2 className="settings-section-title">Look up account</h2>
          <div className="settings-group">
            <div className="settings-row">
              <div className="settings-row-text">
                <div className="settings-row-title">User ID</div>
                <div className="settings-row-desc">
                  Paste someone’s UUID from Profile → User ID
                </div>
              </div>
              <div className="flex items-center gap-2 shrink-0 min-w-0">
                <input
                  value={uuidQuery}
                  onChange={(e) => setUuidQuery(e.target.value)}
                  placeholder="00000000-0000-0000-0000-000000000000"
                  spellCheck={false}
                  className="w-44 max-w-[40vw] px-2.5 py-1.5 rounded-xl bg-[var(--ms-bg)] text-[11px] outline-none font-mono"
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      void loadByUuid();
                    }
                  }}
                />
                <button
                  type="button"
                  disabled={lookupBusy || !uuidQuery.trim()}
                  onClick={() => void loadByUuid()}
                  className="btn-primary px-3 py-1.5 text-xs disabled:opacity-50"
                >
                  {lookupBusy ? "…" : "Load"}
                </button>
              </div>
            </div>
            {lookup ? (
              <>
                <div className="settings-row">
                  <div className="settings-row-text min-w-0">
                    <div className="settings-row-title truncate flex items-center gap-1.5">
                      {lookup.username || lookup.email}
                      {isOwnerAccount(lookup.email) ? (
                        <span className="owner-tag">Owner</span>
                      ) : null}
                    </div>
                    <div className="settings-row-desc truncate">{lookup.email}</div>
                    <div className="settings-row-desc font-mono truncate">{lookup.id}</div>
                  </div>
                </div>
                <div className="settings-row">
                  <div className="settings-row-text">
                    <div className="settings-row-title">Plan</div>
                    <div className="settings-row-desc">Applies on their next sync</div>
                  </div>
                  <div className="flex flex-wrap gap-1.5 justify-end">
                    {PLAN_OPTIONS.map((opt) => {
                      const active = lookup.plan_tier === opt.tier;
                      return (
                        <button
                          key={opt.tier}
                          type="button"
                          disabled={lookupBusy || active}
                          onClick={() => void changeLookupPlan(opt.tier)}
                          className={`text-[11px] px-2.5 py-1 rounded-full ${
                            active
                              ? "bg-[var(--ms-turquoise-glow)] text-[var(--ms-turquoise)]"
                              : "text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)]"
                          }`}
                          style={active ? undefined : { background: "var(--ms-fill-muted)" }}
                        >
                          {opt.label}
                        </button>
                      );
                    })}
                  </div>
                </div>
                <div className="settings-row">
                  <div className="settings-row-text">
                    <div className="settings-row-title">Bonus words</div>
                    <div className="settings-row-desc">
                      Currently +{lookup.usage_bonus.toLocaleString()} this week
                    </div>
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    <input
                      type="number"
                      min={0}
                      step={100}
                      value={lookupGrant}
                      onChange={(e) => setLookupGrant(e.target.value)}
                      className="w-24 px-2.5 py-1.5 rounded-xl bg-[var(--ms-bg)] text-sm outline-none"
                    />
                    <button
                      type="button"
                      disabled={lookupBusy}
                      onClick={() => void addLookupBonus()}
                      className="btn-primary px-3 py-1.5 text-xs disabled:opacity-50"
                    >
                      Add
                    </button>
                  </div>
                </div>
                <div className="settings-row">
                  <div className="settings-row-text">
                    <div className="settings-row-title">Words used this week</div>
                    <div className="settings-row-desc">
                      Sets their meter the next time they open MaxSpeech
                    </div>
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    <input
                      type="number"
                      min={0}
                      value={lookupUsed}
                      onChange={(e) => setLookupUsed(e.target.value)}
                      className="w-24 px-2.5 py-1.5 rounded-xl bg-[var(--ms-bg)] text-sm outline-none"
                    />
                    <button
                      type="button"
                      disabled={lookupBusy}
                      onClick={() => void saveLookupUsed()}
                      className="px-3 py-1.5 text-xs rounded-full font-semibold text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)] disabled:opacity-50"
                      style={{ background: "var(--ms-fill-muted)" }}
                    >
                      Save
                    </button>
                  </div>
                </div>
              </>
            ) : null}
          </div>
        </section>
      )}

      {owner && (
        <section className="space-y-2.5 pb-4">
          <h2 className="settings-section-title">People</h2>
          <div className="settings-group">
            {peopleErr ? (
              <p className="px-4 py-3 text-xs text-[var(--ms-orange)] leading-relaxed">
                {peopleErr}
              </p>
            ) : null}
            {people.length === 0 && !peopleErr ? (
              <p className="px-4 py-3 text-sm text-[var(--ms-text-dim)]">
                No other accounts loaded yet. Signed-in profiles appear here.
              </p>
            ) : (
              people.map((person) => (
                <div key={person.id} className="settings-row settings-row-stack gap-2">
                  <div className="flex items-center justify-between gap-3 w-full">
                    <div className="settings-row-text min-w-0">
                      <div className="settings-row-title truncate flex items-center gap-1.5">
                        {person.username || person.email}
                        {isOwnerAccount(person.email) ? (
                          <span className="owner-tag">Owner</span>
                        ) : null}
                      </div>
                      <div className="settings-row-desc truncate">{person.email}</div>
                      <div className="settings-row-desc font-mono truncate">{person.id}</div>
                    </div>
                    <button
                      type="button"
                      disabled={peopleBusy === person.id}
                      onClick={() => void addPersonBonus(person)}
                      className="px-2.5 py-1 text-[11px] rounded-full font-semibold shrink-0 text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)] disabled:opacity-50"
                      style={{ background: "var(--ms-fill-muted)" }}
                    >
                      +{Number(grant || 0).toLocaleString()} words
                    </button>
                  </div>
                  <div className="flex flex-wrap gap-1.5 w-full">
                    {PLAN_OPTIONS.map((opt) => {
                      const active = person.plan_tier === opt.tier;
                      return (
                        <button
                          key={opt.tier}
                          type="button"
                          disabled={peopleBusy === person.id || active}
                          onClick={() => void changePersonPlan(person, opt.tier)}
                          className={`text-[11px] px-2.5 py-1 rounded-full ${
                            active
                              ? "bg-[var(--ms-turquoise-glow)] text-[var(--ms-turquoise)]"
                              : "text-[var(--ms-text-dim)] hover:text-[var(--ms-hover-fg)]"
                          }`}
                          style={active ? undefined : { background: "var(--ms-fill-muted)" }}
                        >
                          {opt.label}
                        </button>
                      );
                    })}
                    {person.usage_bonus > 0 ? (
                      <span className="text-[11px] text-[var(--ms-text-dim)] px-1 py-1">
                        +{person.usage_bonus.toLocaleString()} bonus
                      </span>
                    ) : null}
                  </div>
                </div>
              ))
            )}
          </div>
        </section>
      )}

      {msg ? (
        <p className="text-xs text-[var(--ms-turquoise)]">{msg}</p>
      ) : null}
    </div>
  );
}
