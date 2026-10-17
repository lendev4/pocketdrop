// "Open with PocketDrop" from the file manager: the files arrive through the
// launch queue and go to the send screen, like an Android share.
(() => {
  const status = document.getElementById("open-status");
  if (!("launchQueue" in window)) {
    status.textContent = "This browser can't open files in PocketDrop. Drag them onto the PocketDrop window instead.";
    return;
  }
  launchQueue.setConsumer(async ({ files }) => {
    if (!files.length) return;
    try {
      const picked = await Promise.all(files.map((handle) => handle.getFile()));
      await ShareStore.save({ title: "", text: "", url: "", files: picked, receivedAt: Date.now() });
      location.replace("/send");
    } catch {
      status.textContent = "Couldn't read the files. Drag them onto the PocketDrop window instead.";
    }
  });
})();
