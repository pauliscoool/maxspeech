// POST /api/stripe-webhook — Stripe events -> profiles.plan_tier.
// Subscribe in Stripe to: checkout.session.completed, customer.subscription.updated,
// customer.subscription.deleted.
const { TIERS, env, stripe, setPlanTier, verifyStripeSignature, readRaw } = require("./_billing");

const ACTIVE = new Set(["active", "trialing", "past_due"]);

/** Plan switches in the customer portal change the price, not our metadata. */
function tierFromPrice(sub) {
  const price = sub.items && sub.items.data && sub.items.data[0] && sub.items.data[0].price && sub.items.data[0].price.id;
  return TIERS.find((t) => price && process.env[`STRIPE_PRICE_${t.toUpperCase()}`] === price);
}

module.exports = async (req, res) => {
  if (req.method !== "POST") {
    res.status(405).end();
    return;
  }
  let event;
  try {
    const raw = await readRaw(req);
    if (!verifyStripeSignature(raw, req.headers["stripe-signature"], env("STRIPE_WEBHOOK_SECRET"))) {
      res.status(400).json({ error: "Bad signature" });
      return;
    }
    event = JSON.parse(raw);
  } catch (e) {
    res.status(e.status || 400).json({ error: e.message });
    return;
  }

  try {
    const obj = event.data && event.data.object;
    if (event.type === "checkout.session.completed") {
      const userId = obj.client_reference_id || (obj.metadata && obj.metadata.user_id);
      const tier = obj.metadata && obj.metadata.tier;
      if (userId && TIERS.includes(tier)) {
        await setPlanTier(userId, tier);
        // Lets the portal endpoint find this customer by account id later.
        if (obj.customer) await stripe("POST", `customers/${obj.customer}`, { metadata: { user_id: userId } });
      }
    } else if (event.type === "customer.subscription.updated" || event.type === "customer.subscription.deleted") {
      const userId = obj.metadata && obj.metadata.user_id;
      const tier = tierFromPrice(obj) || (obj.metadata && obj.metadata.tier);
      if (userId) {
        const keep = event.type === "customer.subscription.updated" && ACTIVE.has(obj.status) && TIERS.includes(tier);
        await setPlanTier(userId, keep ? tier : "free");
      }
    }
    res.status(200).json({ received: true });
  } catch (e) {
    console.error("webhook handling failed:", e.message);
    // 500 makes Stripe retry.
    res.status(500).json({ error: "Handler failed" });
  }
};
