const CACHE = "ms-site-v2";
const PRECACHE = ["/", "/styles.css", "/app.js", "/get.css", "/logo.png", "/logo@2x.png"];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(CACHE).then((cache) => cache.addAll(PRECACHE)).then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))),
    ).then(() => self.clients.claim()),
  );
});

function staleWhileRevalidate(request) {
  return caches.open(CACHE).then((cache) =>
    cache.match(request).then((cached) => {
      const fetched = fetch(request)
        .then((res) => {
          if (res && res.ok) cache.put(request, res.clone());
          return res;
        })
        .catch(() => cached);
      return cached || fetched;
    }),
  );
}

function networkFirst(request) {
  return fetch(request)
    .then((res) => {
      if (res && res.ok) {
        const copy = res.clone();
        caches.open(CACHE).then((cache) => cache.put(request, copy));
      }
      return res;
    })
    .catch(() => caches.match(request));
}

self.addEventListener("fetch", (event) => {
  const req = event.request;
  if (req.method !== "GET") return;

  const url = new URL(req.url);
  const sameOrigin = url.origin === self.location.origin;
  const isFont =
    url.hostname === "fonts.googleapis.com" || url.hostname === "fonts.gstatic.com";

  if (sameOrigin && url.pathname.startsWith("/downloads/")) return;
  if (sameOrigin && url.pathname === "/sw.js") return;

  if (sameOrigin && url.pathname.startsWith("/updates/")) {
    event.respondWith(networkFirst(req));
    return;
  }

  const isDoc =
    req.mode === "navigate" ||
    (sameOrigin && (url.pathname === "/" || url.pathname.endsWith(".html")));

  if (isDoc) {
    event.respondWith(networkFirst(req));
    return;
  }

  if (sameOrigin || isFont) {
    event.respondWith(staleWhileRevalidate(req));
  }
});
