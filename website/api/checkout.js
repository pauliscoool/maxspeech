// POST /api/checkout  (Authorization: Bearer <supabase access token>)
// body {tier: "starter"|"pro"|"max"} -> Stripe Checkout URL
// body {portal: true}                -> Stripe customer portal URL
const { TIERS, env, supabaseUser, stripe, readRaw } = require("./_billing");

const SITE = "https://maxspeech.vercel.app";

async function customerFor(user) {
  const found = await stripe("GET", "customers/search", {
    query: `metadata['user_id']:'${user.id.replace(/'/g, "")}'`,
    limit: 1,
  });
  return found.data && found.data[0] ? found.data[0].id : null;
}

module.exports = async (req, res) => {
  res.setHeader("Cache-Control", "no-store");
  if (req.method !== "POST") {
    res.status(405).json({ error: "POST only" });
    return;
  }
  try {
    const token = (req.headers.authorization || "").replace(/^Bearer\s+/i, "");
    const user = await supabaseUser(token);
    if (!user) {
      res.status(401).json({ error: "Please sign in again from the MaxSpeech app." });
      return;
    }
    let body = {};
    try {
      body = JSON.parse((await readRaw(req)) || "{}");
    } catch {
      res.status(400).json({ error: "Bad request" });
      return;
    }

    if (body.portal) {
      const customer = await customerFor(user);
      if (!customer) {
        res.status(404).json({ error: "No subscription found for this account." });
        return;
      }
      const portal = await stripe("POST", "billing_portal/sessions", { customer, return_url: `${SITE}/checkout?done=1` });
      res.status(200).json({ url: portal.url });
      return;
    }

    const tier = String(body.tier || "").toLowerCase();
    if (!TIERS.includes(tier)) {
      res.status(400).json({ error: "Unknown plan" });
      return;
    }
    const price = env(`STRIPE_PRICE_${tier.toUpperCase()}`);
    const existing = await customerFor(user);
    const session = await stripe("POST", "checkout/sessions", {
      mode: "subscription",
      line_items: [{ price, quantity: 1 }],
      client_reference_id: user.id,
      ...(existing ? { customer: existing } : { customer_email: user.email }),
      metadata: { user_id: user.id, tier },
      subscription_data: { metadata: { user_id: user.id, tier } },
      allow_promotion_codes: "true",
      success_url: `${SITE}/checkout?done=1`,
      cancel_url: `${SITE}/checkout?cancelled=1`,
    });
    res.status(200).json({ url: session.url });
  } catch (e) {
    console.error("checkout failed:", e.message);
    res.status(e.status || 500).json({ error: e.status === 503 ? e.message : "Checkout is unavailable right now." });
  }
};
