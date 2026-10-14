// M0: deliberately minimal. Its only job is making the app installable so it
// appears in Android's Share menu. Share intake moves here in M2.
self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));
