// Send screen for Android shares: shows what the service worker kept, lets
// the user pick an expiry, then uploads it. The share is only forgotten once
// everything is on the server.
(async () => {
  const AUTO_KEY = "pocketdrop.autoSend";
  const section = document.getElementById("share");
  const preview = document.getElementById("share-preview");
  const sendButton = document.getElementById("share-send");
  const auto = document.getElementById("share-auto");
  const expiry = document.getElementById("share-expiry");
  const status = document.getElementById("share-status");
  const list = document.getElementById("uploads");
  const options = { maxFileBytes: Number(section.dataset.maxFile) || 0, pairdropUrl: section.dataset.pairdrop };

  const readAuto = () => {
    try {
      return localStorage.getItem(AUTO_KEY) === "1";
    } catch {
      return false;
    }
  };
  auto.checked = readAuto();
  auto.addEventListener("change", () => {
    try {
      localStorage.setItem(AUTO_KEY, auto.checked ? "1" : "0");
    } catch {}
  });

  let share;
  try {
    share = await ShareStore.load();
  } catch {
    share = null;
  }
  if (!share) {
    preview.innerHTML = '<p class="muted">Nothing waiting to be sent.</p>';
    return;
  }

  // Same rules as the server: many apps put the URL inside the text.
  const text = [share.text.trim(), share.url.trim()]
    .filter((part, index, parts) => part && !(index === 1 && parts[0].includes(part)))
    .join("\n") || (share.files.length ? "" : share.title.trim());

  const items = [];
  if (text) items.push({ text });
  for (const file of share.files) items.push({ file });

  preview.replaceChildren();
  if (text) {
    const block = document.createElement("div");
    block.className = "drop content";
    block.textContent = text;
    preview.append(block);
  }
  for (const file of share.files) {
    const row = document.createElement("div");
    row.className = "file-row drop";
    const name = document.createElement("span");
    name.className = "file-name";
    name.textContent = file.name || "file";
    const size = document.createElement("span");
    size.className = "muted";
    size.textContent = Uploads.formatBytes(file.size);
    row.append(name, size);
    preview.append(row);
  }
  if (!items.length) {
    preview.innerHTML = '<p class="muted">The share contained nothing to send.</p>';
    await ShareStore.clear();
    return;
  }

  const send = async () => {
    sendButton.disabled = true;
    expiry.disabled = true;
    status.textContent = "";
    const results = await Uploads.enqueue(list, items, expiry.value, options);
    await ShareStore.clear();
    preview.replaceChildren();
    status.textContent = results.includes("sent")
      ? "Sent ✓"
      : results.includes("handed-off")
        ? "Continue in PairDrop."
        : "Nothing was sent.";
  };

  sendButton.disabled = false;
  sendButton.addEventListener("click", send);
  if (auto.checked) send();
})();
