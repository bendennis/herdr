import { invoke } from "@tauri-apps/api/core";

let statusEl: HTMLElement | null;
let snapshotEl: HTMLElement | null;

function setStatus(text: string, isError: boolean) {
  if (!statusEl) return;
  statusEl.textContent = text;
  statusEl.classList.toggle("status-error", isError);
  statusEl.classList.toggle("status-ok", !isError);
}

async function connect() {
  setStatus("connecting...", false);
  if (snapshotEl) snapshotEl.textContent = "";

  try {
    await invoke("herdr_ping");
    setStatus("connected (ping ok), fetching snapshot...", false);
  } catch (err) {
    setStatus(`ping failed: ${err}`, true);
    return;
  }

  try {
    const snapshot = await invoke("herdr_snapshot");
    setStatus("connected", false);
    if (snapshotEl) snapshotEl.textContent = JSON.stringify(snapshot, null, 2);
  } catch (err) {
    setStatus(`snapshot failed: ${err}`, true);
  }
}

window.addEventListener("DOMContentLoaded", () => {
  statusEl = document.querySelector("#status");
  snapshotEl = document.querySelector("#snapshot");
  document.querySelector("#connect-button")?.addEventListener("click", () => {
    connect();
  });
});
