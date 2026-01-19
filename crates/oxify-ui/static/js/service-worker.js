// Service Worker for OxiFY UI - Offline Support
// Provides caching strategies for offline access

const CACHE_VERSION = 'oxify-ui-v1';
const STATIC_CACHE = `${CACHE_VERSION}-static`;
const DYNAMIC_CACHE = `${CACHE_VERSION}-dynamic`;
const IMAGE_CACHE = `${CACHE_VERSION}-images`;

// Assets to cache on install
const STATIC_ASSETS = [
    '/static/css/app.css',
    '/static/js/app.js',
    '/static/js/dag-editor.js',
    '/static/js/keyboard-shortcuts.js',
    '/static/js/preferences.js',
    '/',
    '/login',
    '/workflows',
    '/executions',
    '/settings',
];

// Maximum cache size for dynamic content
const MAX_DYNAMIC_CACHE_SIZE = 50;
const MAX_IMAGE_CACHE_SIZE = 30;

// Helper: Limit cache size
async function limitCacheSize(cacheName, maxSize) {
    const cache = await caches.open(cacheName);
    const keys = await cache.keys();
    if (keys.length > maxSize) {
        await cache.delete(keys[0]);
        await limitCacheSize(cacheName, maxSize);
    }
}

// Install event - cache static assets
self.addEventListener('install', (event) => {
    console.log('[Service Worker] Installing service worker...');
    event.waitUntil(
        caches.open(STATIC_CACHE)
            .then((cache) => {
                console.log('[Service Worker] Caching static assets');
                return cache.addAll(STATIC_ASSETS.map(url => new Request(url, {
                    cache: 'reload' // Fetch fresh copies
                })));
            })
            .catch((error) => {
                console.error('[Service Worker] Failed to cache static assets:', error);
            })
    );
    // Force the waiting service worker to become the active service worker
    self.skipWaiting();
});

// Activate event - clean up old caches
self.addEventListener('activate', (event) => {
    console.log('[Service Worker] Activating service worker...');
    event.waitUntil(
        caches.keys().then((cacheNames) => {
            return Promise.all(
                cacheNames
                    .filter((name) => name.startsWith('oxify-ui-') && name !== STATIC_CACHE && name !== DYNAMIC_CACHE && name !== IMAGE_CACHE)
                    .map((name) => {
                        console.log('[Service Worker] Deleting old cache:', name);
                        return caches.delete(name);
                    })
            );
        })
    );
    // Claim all clients immediately
    return self.clients.claim();
});

// Fetch event - caching strategies
self.addEventListener('fetch', (event) => {
    const { request } = event;
    const url = new URL(request.url);

    // Skip non-GET requests
    if (request.method !== 'GET') {
        return;
    }

    // Skip cross-origin requests
    if (url.origin !== self.location.origin) {
        return;
    }

    // Skip SSE endpoints (they need real-time connection)
    if (url.pathname.startsWith('/sse/')) {
        return;
    }

    // Skip API POST/PUT/DELETE requests (handled by HTMX/fetch)
    if (url.pathname.startsWith('/api/v1/')) {
        // Use network-first for API calls
        event.respondWith(
            fetch(request)
                .then((response) => {
                    // Clone response to cache it
                    const responseClone = response.clone();
                    caches.open(DYNAMIC_CACHE).then((cache) => {
                        cache.put(request, responseClone);
                    });
                    return response;
                })
                .catch(() => {
                    // Fallback to cache if offline
                    return caches.match(request);
                })
        );
        return;
    }

    // Cache strategy for static assets (CSS, JS)
    if (url.pathname.startsWith('/static/')) {
        event.respondWith(
            caches.match(request).then((cachedResponse) => {
                if (cachedResponse) {
                    // Return cached version immediately
                    // Update cache in background
                    fetch(request).then((networkResponse) => {
                        caches.open(STATIC_CACHE).then((cache) => {
                            cache.put(request, networkResponse);
                        });
                    }).catch(() => {
                        // Ignore network errors when updating cache
                    });
                    return cachedResponse;
                }

                // Not in cache, fetch from network
                return fetch(request).then((networkResponse) => {
                    const responseClone = networkResponse.clone();
                    caches.open(STATIC_CACHE).then((cache) => {
                        cache.put(request, responseClone);
                    });
                    return networkResponse;
                }).catch((error) => {
                    console.error('[Service Worker] Fetch failed for static asset:', error);
                    throw error;
                });
            })
        );
        return;
    }

    // Cache strategy for images
    if (request.destination === 'image') {
        event.respondWith(
            caches.match(request).then((cachedResponse) => {
                if (cachedResponse) {
                    return cachedResponse;
                }

                return fetch(request).then((networkResponse) => {
                    const responseClone = networkResponse.clone();
                    caches.open(IMAGE_CACHE).then((cache) => {
                        cache.put(request, responseClone);
                        limitCacheSize(IMAGE_CACHE, MAX_IMAGE_CACHE_SIZE);
                    });
                    return networkResponse;
                });
            })
        );
        return;
    }

    // Cache strategy for HTML pages (network-first with cache fallback)
    if (request.headers.get('Accept')?.includes('text/html')) {
        event.respondWith(
            fetch(request)
                .then((response) => {
                    // Clone response to cache it
                    const responseClone = response.clone();
                    caches.open(DYNAMIC_CACHE).then((cache) => {
                        cache.put(request, responseClone);
                        limitCacheSize(DYNAMIC_CACHE, MAX_DYNAMIC_CACHE_SIZE);
                    });
                    return response;
                })
                .catch(() => {
                    // Network failed, try cache
                    return caches.match(request).then((cachedResponse) => {
                        if (cachedResponse) {
                            return cachedResponse;
                        }
                        // No cache available, return offline page
                        return caches.match('/').then((fallback) => {
                            return fallback || new Response('Offline - Please check your connection', {
                                status: 503,
                                statusText: 'Service Unavailable',
                                headers: new Headers({
                                    'Content-Type': 'text/plain'
                                })
                            });
                        });
                    });
                })
        );
        return;
    }

    // Default: network-first strategy
    event.respondWith(
        fetch(request)
            .then((response) => {
                const responseClone = response.clone();
                caches.open(DYNAMIC_CACHE).then((cache) => {
                    cache.put(request, responseClone);
                    limitCacheSize(DYNAMIC_CACHE, MAX_DYNAMIC_CACHE_SIZE);
                });
                return response;
            })
            .catch(() => {
                return caches.match(request);
            })
    );
});

// Message event - handle cache clearing requests
self.addEventListener('message', (event) => {
    if (event.data && event.data.type === 'CLEAR_CACHE') {
        event.waitUntil(
            caches.keys().then((cacheNames) => {
                return Promise.all(
                    cacheNames.map((name) => {
                        console.log('[Service Worker] Clearing cache:', name);
                        return caches.delete(name);
                    })
                );
            }).then(() => {
                event.ports[0].postMessage({ success: true });
            })
        );
    }

    if (event.data && event.data.type === 'SKIP_WAITING') {
        self.skipWaiting();
    }
});
