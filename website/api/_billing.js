// Shared helpers for the Stripe billing endpoints. No npm deps: Stripe's REST API and
// Supabase's auth/REST endpoints are called with fetch.
const crypto = require("crypto");

const SUPABASE_URL = process.env.SUPABASE_URL || "https://eqvmjmejcwkrylqyglfm.supabase.co";
const SUPABASE_ANON_KEY = process.env.SUPABASE_ANON_KEY || "";
const TIERS = ["starter", "pro", "max"];

function env(name) {
  const v = process.env[name];
  if (!v) throw Object.assign(new Error(`Billing is not configured (${name} missing)`), { status: 503 });
  return v;
}

/** The signed-in Supabase user for a bearer token, or null. */
async function supabaseUser(token) {
  if (!token) return null;
  const res = await fetch(`${SUPABASE_URL}/auth/v1/user`, {
    headers: { apikey: SUPABASE_ANON_KEY || env("SUPABASE_SERVICE_ROLE_KEY"), Authorization: `Bearer ${token}` },
  });
  if (!res.ok) return null;
  const user = await res.json();
  return user && user.id ? user : null;
}

async function stripe(method, path, params) {
  const body = params ? new URLSearchParams(flatten(params)).toString() : undefined;
  const url = `https://api.stripe.com/v1/${path}` + (method === "GET" && body ? `?${body}` : "");
  const res = await fetch(url, {
    method,
    headers: {
      Authorization: `Bearer ${env("STRIPE_SECRET_KEY")}`,
      "Content-Type": "application/x-www-form-urlencoded",
    },
    body: method === "GET" ? undefined : body,
  });
  const json = await res.json();
  if (!res.ok) {
    throw Object.assign(new Error(json?.error?.message || `Stripe error ${res.status}`), { status: 502 });
  }
  return json;
}

/** {a: {b: 1}, c: [x]} -> {"a[b]": 1, "c[0]": x} — Stripe's form encoding. */
function flatten(obj, prefix = "", out = {}) {
  for (const [k, v] of Object.entries(obj)) {
    if (v === undefined || v === null) continue;
    const key = prefix ? `${prefix}[${k}]` : k;
    if (typeof v === "object") flatten(v, key, out);
    else out[key] = String(v);
  }
  return out;
}

/** Set profiles.plan_tier with the service-role key (the only role allowed to change it). */
async function setPlanTier(userId, tier) {
  const key = env("SUPABASE_SERVICE_ROLE_KEY");
  const res = await fetch(`${SUPABASE_URL}/rest/v1/profiles?id=eq.${encodeURIComponent(userId)}`, {
    method: "PATCH",
    headers: {
      apikey: key,
      Authorization: `Bearer ${key}`,
      "Content-Type": "application/json",
      Prefer: "return=minimal",
    },
    body: JSON.stringify({ plan_tier: tier }),
  });
  if (!res.ok) throw new Error(`Could not update plan (${res.status}): ${await res.text()}`);
}

/** Stripe-Signature check (v1 HMAC-SHA256 over "t.payload"), 5-minute tolerance. */
function verifyStripeSignature(rawBody, header, secret) {
  if (!header) return false;
  const parts = Object.fromEntries(
    header.split(",").map((p) => {
      const i = p.indexOf("=");
      return [p.slice(0, i), p.slice(i + 1)];
    }),
  );
  const t = Number(parts.t);
  if (!t || Math.abs(Date.now() / 1000 - t) > 300) return false;
  const expected = crypto.createHmac("sha256", secret).update(`${t}.${rawBody}`).digest("hex");
  const sigs = header
    .split(",")
    .filter((p) => p.startsWith("v1="))
    .map((p) => p.slice(3));
  return sigs.some((s) => s.length === expected.length && crypto.timingSafeEqual(Buffer.from(s), Buffer.from(expected)));
}

function readRaw(req) {
  return new Promise((resolve, reject) => {
    let data = "";
    req.setEncoding("utf8");
    req.on("data", (c) => (data += c));
    req.on("end", () => resolve(data));
    req.on("error", reject);
  });
}

module.exports = { TIERS, env, supabaseUser, stripe, setPlanTier, verifyStripeSignature, readRaw };
