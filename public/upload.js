// Upload queue: each text or file is its own request, with a progress bar,
// cancel and retry. Results show up in the drop list via the live stream.
self.Uploads = (() => {
  function formatBytes(bytes) {
    if (bytes < 1024) return `${bytes} B`;
    const units = ["KB", "MB", "GB"];
    let value = bytes / 1024;
    let unit = 0;
    while (value >= 1024 && unit < units.length - 1) {
      value /= 1024;
      unit += 1;
    }
    return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
  }

  function send(item, expiry, onProgress) {
    const xhr = new XMLHttpRequest();
    const done = new Promise((resolve, reject) => {
      const body = new FormData();
      // Expiry first: the server applies it to the parts after it.
      body.append("expiry", expiry);
      if (item.file) body.append("files", item.file, item.file.name);
      else body.append("text", item.text);

      xhr.open("POST", "/drops");
      xhr.setRequestHeader("Accept", "application/json");
      xhr.upload.onprogress = (event) => {
        if (event.lengthComputable) onProgress(event.loaded / event.total);
      };
      xhr.onload = () =>
        xhr.status >= 200 && xhr.status < 300
          ? resolve()
          : reject(new Error(xhr.responseText || `Failed (${xhr.status})`));
      xhr.onerror = () => reject(new Error("Network error. Check the connection and retry."));
      xhr.onabort = () => reject(new Error("Cancelled"));
      xhr.send(body);
    });
    return { done, cancel: () => xhr.abort() };
  }

  /**
   * Opens PairDrop in a new tab with the file already loaded. This posts the
   * file the way Android's share menu does; PairDrop's service worker picks
   * it up, so PairDrop must have been opened in this browser before.
   */
  function openInPairDrop(file, pairdropUrl) {
    const form = document.createElement("form");
    form.method = "POST";
    form.action = pairdropUrl;
    form.enctype = "multipart/form-data";
    form.target = "_blank";
    form.hidden = true;
    const input = document.createElement("input");
    input.type = "file";
    input.name = "allfiles";
    const transfer = new DataTransfer();
    transfer.items.add(file);
    input.files = transfer.files;
    form.append(input);
    document.body.append(form);
    form.submit();
    setTimeout(() => form.remove(), 1000);

    // Tell the other devices to open PairDrop to receive it.
    fetch("/drops/handoff", {
      method: "POST",
      credentials: "same-origin",
      body: new URLSearchParams({ name: file.name, size: String(file.size) }),
    });
  }

  /**
   * Uploads items ({ text } or { file }) into the given list element.
   * options: { maxFileBytes, pairdropUrl }. Resolves once every item has been
   * uploaded (after retries), handed to PairDrop or removed.
   */
  function enqueue(list, items, expiry, options = {}) {
    return Promise.all(items.map((item) => enqueueOne(list, item, expiry, options)));
  }

  function enqueueOne(list, item, expiry, { maxFileBytes, pairdropUrl } = {}) {
    const row = document.createElement("li");
    row.className = "upload";
    const label = item.file
      ? `${item.file.name} · ${formatBytes(item.file.size)}`
      : item.text.length > 60 ? `${item.text.slice(0, 60)}…` : item.text;
    row.innerHTML = `
      <div class="upload-row"><span class="upload-name"></span><span class="upload-state"></span></div>
      <progress max="1" value="0"></progress>
      <div class="upload-actions">
        <button type="button" data-action="pairdrop" hidden>Send with PairDrop</button>
        <button type="button" class="quiet" data-action="retry" hidden>Retry</button>
        <button type="button" class="quiet" data-action="cancel">Cancel</button>
      </div>`;
    row.querySelector(".upload-name").textContent = label;
    const state = row.querySelector(".upload-state");
    const bar = row.querySelector("progress");
    const retry = row.querySelector('[data-action="retry"]');
    const cancel = row.querySelector('[data-action="cancel"]');
    const pairdrop = row.querySelector('[data-action="pairdrop"]');
    list.append(row);

    return new Promise((resolve) => {
      let current = null;

      const finish = (result) => {
        resolve(result);
      };

      const attempt = () => {
        retry.hidden = true;
        cancel.textContent = "Cancel";
        row.classList.remove("failed");
        if (item.file && maxFileBytes && item.file.size > maxFileBytes) {
          fail(new Error(`Over the ${formatBytes(maxFileBytes)} limit.`), false);
          pairdrop.hidden = !pairdropUrl;
          return;
        }
        state.textContent = "Uploading…";
        bar.value = 0;
        current = send(item, expiry, (fraction) => {
          bar.value = fraction;
          state.textContent = `${Math.round(fraction * 100)}%`;
        });
        current.done.then(
          () => {
            state.textContent = "Sent ✓";
            bar.value = 1;
            row.classList.add("sent");
            cancel.hidden = true;
            setTimeout(() => row.remove(), 2500);
            finish("sent");
          },
          (error) => {
            if (error.message === "Cancelled") return;
            fail(error, true);
          },
        );
      };

      const fail = (error, retryable) => {
        current = null;
        row.classList.add("failed");
        state.textContent = error.message;
        retry.hidden = !retryable;
        cancel.textContent = "Remove";
      };

      retry.addEventListener("click", attempt);
      pairdrop.addEventListener("click", () => {
        openInPairDrop(item.file, pairdropUrl);
        row.classList.remove("failed");
        state.textContent = "Opened in PairDrop: pick your device there.";
        pairdrop.hidden = true;
        cancel.textContent = "Dismiss";
        finish("handed-off");
      });
      cancel.addEventListener("click", () => {
        if (current) current.cancel();
        row.remove();
        finish("removed");
      });

      attempt();
    });
  }

  return { enqueue, formatBytes };
})();

// Main page: send form, drag-and-drop and paste anywhere.
document.addEventListener("DOMContentLoaded", () => {
  const form = document.getElementById("send-form");
  if (!form) return;
  const list = document.getElementById("uploads");
  const textarea = form.querySelector("textarea");
  const picker = form.querySelector('input[type="file"]');
  const options = { maxFileBytes: Number(form.dataset.maxFile) || 0, pairdropUrl: form.dataset.pairdrop };
  const expiry = () => form.querySelector('select[name="expiry"]').value;

  const upload = (items) => {
    if (items.length) Uploads.enqueue(list, items, expiry(), options);
  };

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const items = [];
    if (textarea.value.trim()) items.push({ text: textarea.value });
    for (const file of picker.files) items.push({ file });
    upload(items);
    textarea.value = "";
    picker.value = "";
  });

  // Ctrl/Cmd+Enter sends from the text box.
  textarea.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) form.requestSubmit();
  });

  // Picking files sends them right away.
  picker.addEventListener("change", () => {
    upload([...picker.files].map((file) => ({ file })));
    picker.value = "";
  });

  document.addEventListener("dragover", (event) => {
    if (!event.dataTransfer?.types.includes("Files")) return;
    event.preventDefault();
    document.body.classList.add("dragging");
  });
  document.addEventListener("dragleave", (event) => {
    if (!event.relatedTarget) document.body.classList.remove("dragging");
  });
  document.addEventListener("drop", (event) => {
    if (!event.dataTransfer?.files.length) return;
    event.preventDefault();
    document.body.classList.remove("dragging");
    upload([...event.dataTransfer.files].map((file) => ({ file })));
  });

  // Pasted files (e.g. a screenshot) are sent right away. Pasted text is
  // sent too, unless it goes into a text field.
  document.addEventListener("paste", (event) => {
    const files = [...(event.clipboardData?.files ?? [])];
    const text = event.clipboardData?.getData("text") ?? "";
    if (files.length) upload(files.map((file) => ({ file })));
    else if (text.trim() && !event.target.closest("textarea, input")) upload([{ text }]);
    else return;
    event.preventDefault();
  });
});
