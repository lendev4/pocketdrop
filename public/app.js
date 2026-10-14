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
