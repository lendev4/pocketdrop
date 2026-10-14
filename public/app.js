if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("/sw.js");
}

// A `once` drop is used up by copying its text or opening its link.
function consume(element) {
  const id = element.closest("[data-once]")?.dataset.once;
  if (id) fetch(`/drops/${id}/consume`, { method: "POST", credentials: "same-origin" });
}

// Closes the pairing panel.
document.addEventListener("click", (event) => {
  if (event.target.closest("[data-close-pair]")) document.getElementById("pair")?.replaceChildren();
});

// Copy buttons carry the text to copy in data-copy.
document.addEventListener("click", async (event) => {
  const link = event.target.closest("a.content, a[data-consume]");
  if (link) consume(link);
  const button = event.target.closest("[data-copy]");
  if (!button) return;
  await navigator.clipboard.writeText(button.dataset.copy);
  const label = button.textContent;
  button.textContent = "Copied ✓";
  setTimeout(() => (button.textContent = label), 1200);
  consume(button);
});

// A share kept on the phone but never sent, e.g. after logging in again.
document.addEventListener("DOMContentLoaded", async () => {
  const banner = document.getElementById("pending-share");
  if (!banner || !self.ShareStore) return;
  try {
    if (await ShareStore.load()) banner.hidden = false;
  } catch {}
});
