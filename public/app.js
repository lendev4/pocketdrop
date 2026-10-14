if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("/sw.js");
}

function storage(action) {
  try {
    return action(localStorage);
  } catch {
    return null;
  }
}

// "Keep for" is remembered per device. Runs right away (scripts are
// deferred) so the share screen sees the remembered value before it sends.
const EXPIRY_KEY = "pocketdrop.expiry";
for (const select of document.querySelectorAll('select[name="expiry"]')) {
  const saved = storage((s) => s.getItem(EXPIRY_KEY));
  if (saved && [...select.options].some((option) => option.value === saved)) select.value = saved;
  select.addEventListener("change", () => storage((s) => s.setItem(EXPIRY_KEY, select.value)));
}

// A `once` drop is used up by copying it or opening its link.
function consume(element) {
  const id = element.closest("[data-once]")?.dataset.once;
  if (id) fetch(`/drops/${id}/consume`, { method: "POST", credentials: "same-origin" });
}

function flash(button, text) {
  const label = button.dataset.label ?? button.textContent;
  button.dataset.label = label;
  button.textContent = text;
  setTimeout(() => (button.textContent = label), 1500);
}

/** PNG of an image URL: the only image type browsers put on the clipboard. */
async function pngBlob(url) {
  const response = await fetch(url, { credentials: "same-origin" });
  const blob = await response.blob();
  if (blob.type === "image/png") return blob;
  const bitmap = await createImageBitmap(blob);
  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  canvas.getContext("2d").drawImage(bitmap, 0, 0);
  return new Promise((resolve) => canvas.toBlob(resolve, "image/png"));
}

let clearArmed = null;

document.addEventListener("click", async (event) => {
  const target = event.target;

  if (target.closest("[data-close-pair]")) document.getElementById("pair")?.replaceChildren();

  // Closes the header menu after choosing an entry.
  if (target.closest("[data-close-menu]")) target.closest("details")?.removeAttribute("open");

  const link = target.closest("a.content, a[data-consume]");
  if (link) consume(link);

  // Clear all asks for a second tap instead of a browser dialog.
  const clear = target.closest("[data-clear-all]");
  if (clear) {
    if (clearArmed === clear) {
      clearTimeout(clear.disarm);
      clearArmed = null;
      fetch("/drops", { method: "DELETE", credentials: "same-origin" });
    } else {
      clearArmed = clear;
      clear.classList.add("armed");
      clear.textContent = "Tap again to clear all";
      clear.disarm = setTimeout(() => {
        clearArmed = null;
        clear.classList.remove("armed");
        clear.textContent = "Clear all";
      }, 3000);
    }
    return;
  }

  const copyText = target.closest("[data-copy]");
  if (copyText) {
    await navigator.clipboard.writeText(copyText.dataset.copy);
    flash(copyText, "Copied ✓");
    consume(copyText);
    return;
  }

  const copyImage = target.closest("[data-copy-image]");
  if (copyImage) {
    try {
      // Handing the clipboard a promise keeps the click's permission while
      // the image downloads.
      await navigator.clipboard.write([new ClipboardItem({ "image/png": pngBlob(copyImage.dataset.copyImage) })]);
      flash(copyImage, "Copied ✓");
      consume(copyImage);
    } catch {
      flash(copyImage, "Can't copy here");
    }
  }
});

// Close the menu when tapping elsewhere.
document.addEventListener("click", (event) => {
  for (const menu of document.querySelectorAll("details.menu[open]")) {
    if (!menu.contains(event.target)) menu.removeAttribute("open");
  }
});

// Keeps "5 min ago" and "expires in 9 min" current between list updates.
function durationText(secs) {
  if (secs < 3600) return `${Math.max(1, Math.floor(secs / 60))} min`;
  if (secs < 172800) {
    const hours = Math.floor(secs / 3600);
    return `${hours} hour${hours === 1 ? "" : "s"}`;
  }
  const days = Math.floor(secs / 86400);
  return `${days} day${days === 1 ? "" : "s"}`;
}

function refreshTimes() {
  const now = Date.now() / 1000;
  for (const el of document.querySelectorAll("[data-age]")) {
    const secs = now - Number(el.dataset.age);
    el.textContent = secs < 60 ? "just now" : `${durationText(secs)} ago`;
  }
  for (const el of document.querySelectorAll("[data-expires]")) {
    el.textContent = `expires in ${durationText(Math.max(0, Number(el.dataset.expires) - now))}`;
  }
}
setInterval(refreshTimes, 30000);
document.addEventListener("visibilitychange", () => document.visibilityState === "visible" && refreshTimes());

// Briefly highlights drops that arrive while the page is open.
const seen = new Set([...document.querySelectorAll("li.drop[id]")].map((li) => li.id));
new MutationObserver(() => {
  for (const li of document.querySelectorAll("li.drop[id]")) {
    if (seen.has(li.id)) continue;
    seen.add(li.id);
    li.classList.add("fresh");
    setTimeout(() => li.classList.remove("fresh"), 2500);
  }
}).observe(document.body, { childList: true, subtree: true });

// A share kept on the phone but never sent, e.g. after logging in again.
(async () => {
  const banner = document.getElementById("pending-share");
  if (!banner || !self.ShareStore) return;
  try {
    if (await ShareStore.load()) banner.hidden = false;
  } catch {}
})();
