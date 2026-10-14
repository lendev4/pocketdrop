// Keeps one pending Android share in IndexedDB until it has been uploaded.
// Loaded by the service worker (importScripts) and by every page.
self.ShareStore = (() => {
  const DB = "pocketdrop";
  const STORE = "shares";
  const KEY = "pending";

  function open() {
    return new Promise((resolve, reject) => {
      const request = indexedDB.open(DB, 1);
      request.onupgradeneeded = () => request.result.createObjectStore(STORE);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
  }

  async function run(mode, action) {
    const db = await open();
    try {
      return await new Promise((resolve, reject) => {
        const tx = db.transaction(STORE, mode);
        const request = action(tx.objectStore(STORE));
        tx.oncomplete = () => resolve(request.result);
        tx.onerror = () => reject(tx.error);
      });
    } finally {
      db.close();
    }
  }

  return {
    /** share: { title, text, url, files: File[], receivedAt } */
    save: (share) => run("readwrite", (store) => store.put(share, KEY)),
    load: () => run("readonly", (store) => store.get(KEY)),
    clear: () => run("readwrite", (store) => store.delete(KEY)),
  };
})();
