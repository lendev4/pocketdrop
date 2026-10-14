if ("serviceWorker" in navigator) {
  navigator.serviceWorker.register("/sw.js");
}

// Copy buttons carry the text to copy in data-copy.
document.addEventListener("click", async (event) => {
  const button = event.target.closest("[data-copy]");
  if (!button) return;
  await navigator.clipboard.writeText(button.dataset.copy);
  const label = button.textContent;
  button.textContent = "Copied ✓";
  setTimeout(() => (button.textContent = label), 1200);
});

// A share kept on the phone but never sent, e.g. after logging in again.
document.addEventListener("DOMContentLoaded", async () => {
  const banner = document.getElementById("pending-share");
  if (!banner || !self.ShareStore) return;
  try {
    if (await ShareStore.load()) banner.hidden = false;
  } catch {}
});
