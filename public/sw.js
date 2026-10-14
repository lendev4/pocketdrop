// Receives Android shares: keeps the shared content on the phone and opens
// the send screen, which uploads it with progress and retry. Nothing else is
// intercepted or cached.
importScripts("/share-store.js");

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);
  if (event.request.method === "POST" && url.origin === self.location.origin && url.pathname === "/share") {
    event.respondWith(receiveShare(event.request));
  }
});

async function receiveShare(request) {
  const form = await request.formData();
  const share = {
    title: form.get("title") || "",
    text: form.get("text") || "",
    url: form.get("url") || "",
    files: form.getAll("files").filter((file) => file instanceof File && (file.size > 0 || file.name)),
    receivedAt: Date.now(),
  };
  try {
    await ShareStore.save(share);
    return Response.redirect(new URL("/send", self.location.origin).href, 303);
  } catch (error) {
    // No storage available: let the server take the share directly.
    return fetch("/share", { method: "POST", body: form, credentials: "same-origin" });
  }
}
