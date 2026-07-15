/* Glossia service worker — offline app shell for the browser UIs.
 *
 * Strategy:
 *   - Precache the app shell on install (resilient: a missing entry, e.g. an
 *     unbuilt WASM artifact, does not abort the install).
 *   - Same-origin GET requests are served stale-while-revalidate: the cached
 *     copy answers instantly (offline-capable) while a background fetch refreshes
 *     the cache for next time. This lets rebuilt JS/WASM propagate on the next
 *     visit without any manual cache-busting.
 *   - Navigations fall back to the cached page, then to index.html, when offline.
 *   - Cross-origin requests (nostr relays over wss://, remote images) are left
 *     untouched — the SW never intercepts them.
 *
 * Bump CACHE when the shell list changes or you want to force-evict old caches.
 * Everything here is scoped to the directory sw.js is served from, so it works
 * unchanged at the site root (glossia.io) and under a per-PR preview subpath.
 */
const CACHE = 'glossia-shell-v1';

// App shell, relative to the SW scope. glossia.js / glossia_bg.wasm are
// gitignored build artifacts — present after a build/deploy, possibly absent in
// a bare checkout — so precaching tolerates their absence (see install below).
const SHELL = [
  './',
  './index.html',
  './compose.html',
  './bulletin.html',
  './glossia.js',
  './glossia_bg.wasm',
  './glossia-msg.js',
  './glossia-nostr.js',
  './favicon.svg',
  './manifest.webmanifest',
  './icons/glossia-icon-16.png',
  './icons/glossia-icon-32.png',
  './icons/glossia-icon-180.png',
  './icons/glossia-icon-192.png',
  './icons/glossia-icon-512.png',
  './icons/glossia-icon.svg',
  './vendor/noble/curves/secp256k1.js',
  './vendor/noble/curves/shortw_utils.js',
  './vendor/noble/curves/abstract/hash-to-curve.js',
  './vendor/noble/curves/abstract/curve.js',
  './vendor/noble/curves/abstract/utils.js',
  './vendor/noble/curves/abstract/weierstrass.js',
  './vendor/noble/curves/abstract/modular.js',
  './vendor/noble/hashes/assert.js',
  './vendor/noble/hashes/sha256.js',
  './vendor/noble/hashes/hmac.js',
  './vendor/noble/hashes/crypto.js',
  './vendor/noble/hashes/utils.js',
  './vendor/noble/hashes/md.js',
];

self.addEventListener('install', (event) => {
  event.waitUntil((async () => {
    const cache = await caches.open(CACHE);
    // Add entries individually so one 404 (e.g. an unbuilt artifact) doesn't
    // reject the whole install — anything missing is filled in at runtime.
    await Promise.allSettled(SHELL.map((url) => cache.add(new Request(url, { cache: 'reload' }))));
    await self.skipWaiting();
  })());
});

self.addEventListener('activate', (event) => {
  event.waitUntil((async () => {
    const keys = await caches.keys();
    await Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k)));
    await self.clients.claim();
  })());
});

self.addEventListener('fetch', (event) => {
  const req = event.request;
  if (req.method !== 'GET') return;

  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return; // don't touch cross-origin (relays, remote images)

  event.respondWith((async () => {
    const cache = await caches.open(CACHE);
    const cached = await cache.match(req);

    const network = fetch(req).then((res) => {
      // Only cache complete, same-origin OK responses.
      if (res && res.ok && res.type === 'basic') cache.put(req, res.clone());
      return res;
    }).catch(() => null);

    // Stale-while-revalidate: cached copy now, refresh in the background.
    if (cached) {
      event.waitUntil(network);
      return cached;
    }

    const res = await network;
    if (res) return res;

    // Offline and uncached: fall back to a shell page for navigations.
    if (req.mode === 'navigate') {
      return (await cache.match(req)) ||
             (await cache.match('./index.html')) ||
             (await cache.match('./')) ||
             Response.error();
    }
    return Response.error();
  })());
});
