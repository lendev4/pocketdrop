# PocketDrop — Specification

**Status:** Draft v1 · 2026-10-14
**Supersedes:** the earlier ChatGPT outline (`domain.md`, removed)

## 1. What it is

A private, single-user **drop zone** for moving text, links and files between your own devices, mostly Android → Linux and sometimes the other way.

You share something from Android's Share menu, or paste or drag it into the page on Linux. It shows up live on every other open device. You copy or download it, and it disappears again on its own.

It is **not** an archive or an inbox to organise. Drops are short-lived and are usually needed once.

### Principles

- **Few taps.** Android Share → PocketDrop → sent.
- **Asynchronous.** The receiving device can be offline. Drops wait on the server until they expire.
- **Live when open.** An open page updates instantly when a drop arrives.
- **No native apps, no daemons.** It's a website, installable as a PWA.
- **Tiny footprint.** One small binary, one folder of data, no database.
- **Self-contained per person.** Anyone (e.g. a friend) can deploy their own instance from the Dockerfile.

### Out of scope

- Multiple users or accounts, sharing with other people, public links.
- Backups, history, search, organising drops.
- Clipboard sync or background syncing.
- Peer-to-peer transfer of very large files. At most a link to an external tool such as PairDrop.
- End-to-end encryption. Content is protected by HTTPS and the login only, and this is stated plainly.

## 2. Stack

| Area | Choice |
|---|---|
| Language / framework | Rust, [Topcoat](https://github.com/tokio-rs/topcoat) **0.10, pinned exactly** (experimental, breaking changes expected) |
| Rendering | Server-rendered HTML via Topcoat's `view!` |
| Interactivity | Datastar 1.0.x (`datastar` feature), served as a bundled asset rather than from a CDN at runtime |
| Realtime | Server-sent events (SSE) plus a `tokio::sync::broadcast` "drops changed" channel |
| Storage | Plain files on disk: one folder per drop. No database. |
| Own client JS | `upload.js` (progress, drag-and-drop, paste) and `sw.js` (Android share intake) |
| Hosting | Railway, single always-on instance, volume mounted at `/data` |
| Deploy | Multi-stage Dockerfile producing a small single-binary image |

## 3. Concepts

- **Drop:** one item of kind `text`, `link` or `file`. It has an id, a creation time, an expiry rule and its content.
- **Expiry rule:** decides when a drop is deleted, either at a time or after it has been consumed once.
- **Device:** any browser holding a valid login cookie. Devices are not tracked server-side.

## 4. User flows

### 4.1 Send from Android (main use case)

1. Select text, a link or a file in any app → **Share** → **PocketDrop**.
2. The service worker receives the share and opens the **send screen**.
3. What happens next depends on the per-device setting **"Confirm before sending"**:
   - **On (default):** the screen shows a preview plus an expiry choice (§5), and you tap **Send**.
   - **Off:** it sends immediately with the default expiry.
4. A progress bar shows the upload. Success is shown only after the server has stored the drop.
5. If the upload fails, the content stays on the phone with a **Retry** button.

### 4.2 Send from Linux (or any open page)

- **Paste:** Ctrl+V anywhere on the page sends clipboard text, a link or an image.
- **Drag-and-drop** files onto the page, or use a **text box** or **file picker**.
- An expiry choice sits next to the send action and defaults to the configured default.
- A progress bar is shown, with cancel and retry.

### 4.3 Receive

- The main page lists current drops, newest first. It updates live over SSE, with no refresh needed.
- Each drop shows its kind, its age and when it expires.
  - **Text:** a preview (expandable) and a **Copy** button.
  - **Link:** **Open** (only `http`/`https`) and **Copy**.
  - **File:** name, size, a preview if it's a JPEG/PNG/GIF/WebP/AVIF image (the original, shown inline and lazy-loaded), and **Download**.
- **Delete** on every drop, plus **Clear all**.

### 4.4 Log in, pair, log out

- **Log in:** enter the passphrase on the login page. A long-lived cookie is set (§6.1).
- **Pair a device:** on a logged-in device choose **Pair device**. Scan the QR code with the other device and it's logged in (§6.2).
- **Log out:** removes the cookie on that device only.
- **Log out everywhere:** rotate `SESSION_SECRET` and redeploy (§6.1).

## 5. Expiry

Every drop has one expiry rule, chosen when sending:

| Option | Meaning |
|---|---|
| `10m`, `1h`, `24h`, `7d` | Deleted at `created_at + duration` |
| `never` | Kept until deleted manually |
| `once` | Deleted after it has been consumed once (see below) |

- **Default:** set by `DEFAULT_EXPIRY` (initially `24h`).
- **Override:** each send can pick a different option. The choices offered are configurable via `EXPIRY_OPTIONS`.
- **What counts as consumed for `once`:**
  - **Files:** a completed download.
  - **Text and links:** pressing Copy or Open, which notifies the server.

  The drop is then marked consumed. It disappears from the list immediately and is physically deleted after a short grace period (`ONCE_GRACE`, default 5 min), so an interrupted download can be retried.
- **Enforcement:**
  - Expired drops are hidden and refused immediately, even before physical deletion.
  - A cleanup task runs every minute and at startup. It deletes expired and consumed drops and enforces the limits in §7.

## 6. Authentication

### 6.1 Passphrase plus signed cookie

- The `PASSPHRASE` environment variable holds the only password.
- A successful login sets a cookie signed with `SESSION_SECRET`, valid for `SESSION_DAYS` (default 365).
  - Flags: `HttpOnly`, `Secure`, `SameSite=Lax`.
  - Nothing is stored server-side. The server only checks the signature and age.
- **Forged requests from other sites:** every state-changing request must have an `Origin` header matching the site, plus `SameSite`.
- **Brute force:** login attempts are rate-limited per IP, e.g. 5 per minute, held in memory.
- **Revoking access:** changing `SESSION_SECRET` invalidates every cookie on every device.

### 6.2 QR pairing

1. A logged-in device requests **Pair device**. The server creates a random **pairing token** and keeps it in memory, single-use, valid 5 minutes.
2. The page shows a QR code for `https://<host>/pair/<token>`, plus the URL as text for manual entry.
3. The new device opens the URL. The server checks the token, deletes it and sets the login cookie.
4. Over SSE, the first device shows **"Device paired ✓"** and closes the QR code.

Anyone who sees the QR code within those 5 minutes could pair a device. That's acceptable for a personal tool. Restarting the server invalidates any pending pairing tokens.

### 6.3 API token (optional)

If `API_TOKEN` is set, requests to `/api/*` with `Authorization: Bearer <API_TOKEN>` are accepted without a cookie, for scripts and terminal use:

```sh
curl -H "Authorization: Bearer $T" -F file=@report.pdf https://<host>/api/drops
curl -H "Authorization: Bearer $T" -F text="hello"     https://<host>/api/drops
curl -H "Authorization: Bearer $T" https://<host>/api/latest   # prints the newest text, or downloads the newest file
```

## 7. Limits

All limits are configurable. The server enforces them and never trusts the client.

| Setting | Default | Notes |
|---|---|---|
| `MAX_FILE_BYTES` | 100 MB | Checked while streaming; the upload is aborted once exceeded |
| `MAX_TEXT_BYTES` | 1 MB | |
| `MAX_TOTAL_BYTES` | 2 GB | Uploads that would exceed it are rejected with a clear message |
| `MAX_DROPS` | 50 | Above this the oldest drops are removed, except `never` drops |

## 8. Storage

```
/data/
  drops/
    01JA8X…/                 ← id: time-sortable (ULID), so the folder name orders by time
      meta.json
      content                ← the text, or the file's bytes
  tmp/                       ← uploads in progress
```

`meta.json`:

```json
{
  "kind": "file",
  "created_at": "2026-10-14T21:03:12Z",
  "expiry": "24h",
  "expires_at": "2026-10-15T21:03:12Z",
  "consumed_at": null,
  "file_name": "report.pdf",
  "mime": "application/pdf",
  "size": 482113,
  "source": "android"
}
```

Rules:

- **Atomic writes:** uploads are written into `tmp/<id>/` and **renamed** into `drops/` only after they finish completely. A broken upload therefore never appears as a drop. `tmp/` is emptied at startup.
- **File names** are display text only. Stored bytes are always at `content`, never at a path derived from user input.
- **Listing** reads `drops/`, sorts by id descending and skips expired or consumed drops. At this size no index is needed.
- **Total size** is computed at startup and kept in memory, updated on every write and delete.

## 9. Realtime

- **On page load**, the main page connects to `GET /events`, an SSE stream started with Datastar's `@get`.
- **On every change** (create, consume, delete, expire, pairing done), the server broadcasts on the channel. Each open stream sends a Datastar `PatchElements` event with freshly rendered `#drops` HTML.
- **On connect and reconnect**, the full list is sent, so a sleeping laptop or background tab catches up automatically.
- **Keep-alive:** sent about every 25 s so Railway's proxy doesn't close idle connections.
- **One instance only**, because the broadcast channel lives in that process's memory.

## 10. Client JavaScript

### `upload.js`

- Sends text and files to `POST /drops` using `XMLHttpRequest`, which provides upload progress.
- Shows progress, cancel and retry.
- Handles paste anywhere on the page (text, URLs and images from the clipboard), drag-and-drop and the file picker.
- Sends the chosen expiry with each drop.

### `sw.js` and the manifest

- The manifest `share_target` is `POST /share`, `multipart/form-data`, with params `title`, `text`, `url` and `files` (accepting `*/*`).
- **When the service worker is active**, it intercepts `POST /share`:
  1. It stores the payload in IndexedDB.
  2. It redirects to `/send`.
  3. The send screen reads the payload and either shows the confirm screen or sends immediately.
  4. It uploads via `upload.js`.
  5. It clears the stored payload only after the server confirms.
- **Fallback when the service worker isn't active:** the server also accepts `POST /share` directly. It stores the drop with the default expiry and shows "Sent ✓", with no progress bar.
- Beyond that the service worker caches **nothing**, so there are no stale-version problems.
- The "Confirm before sending" setting is per device, stored in `localStorage`.

### Everything else

Live list, Copy, Delete, Clear all and the pairing dialog are Datastar attributes in the server-rendered HTML. No custom script is needed for them.

## 11. Routes

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/` | Main page: drop list and send area |
| `GET` / `POST` | `/login` | Login form / check passphrase |
| `POST` | `/logout` | Delete the cookie on this device |
| `GET` | `/events` | SSE stream (Datastar patches) |
| `POST` | `/drops` | Create a drop (multipart: `text` or `file`, plus `expiry`) |
| `GET` | `/drops/:id/download` | Download a file (`Content-Disposition: attachment`) |
| `GET` | `/drops/:id/file?inline` | Inline preview, only for raster image types (never SVG) |
| `POST` | `/drops/:id/consume` | Mark a `once` text or link as consumed |
| `DELETE` | `/drops/:id` | Delete one drop |
| `DELETE` | `/drops` | Clear all |
| `POST` | `/share` | Android Share Target (fallback path; normally handled by the service worker) |
| `GET` | `/send` | Share confirm / send screen |
| `POST` | `/pair` | Create a pairing token, returns the QR dialog |
| `GET` | `/pair/:token` | Use a pairing token, sets the cookie |
| `POST` | `/api/drops`, `GET` `/api/latest` | Bearer-token API (§6.3) |
| `GET` | `/manifest.webmanifest`, `/sw.js`, `/icons/*` | PWA files, served from `public/` at the site root |
| `GET` | `/healthz` | Health check for Railway |

Every route except `/login`, `/pair/:token`, the PWA files and `/healthz` requires authentication.

## 12. Security checklist

- All user content is escaped. `view!` escapes by default, and raw HTML is never rendered.
- Links are clickable only for `http:` and `https:`. Anything else is shown as plain text.
- **Downloads:**
  - Always sent with `Content-Disposition: attachment` and a sanitised file name.
  - `X-Content-Type-Options: nosniff`.
  - Never rendered inline, which avoids running uploaded HTML or SVG as part of the site.
- Response headers: Content-Security-Policy (`script-src 'self'`; check that Datastar works within it), `Referrer-Policy: no-referrer`.
- Logs never contain drop content, file names, cookies, tokens or the passphrase.

## 13. Configuration

| Variable | Required | Default |
|---|---|---|
| `PASSPHRASE` | yes | |
| `SESSION_SECRET` | yes | (≥ 64 random bytes) |
| `SESSION_DAYS` | | `365` |
| `API_TOKEN` | | unset, which disables the API |
| `DATA_DIR` | | `/data` |
| `DEFAULT_EXPIRY` | | `24h` |
| `EXPIRY_OPTIONS` | | `10m,1h,24h,7d,never` (`once` added in M3) |
| `ONCE_GRACE` | | `5m` |
| `MAX_FILE_BYTES` / `MAX_TEXT_BYTES` / `MAX_TOTAL_BYTES` / `MAX_DROPS` | | see §7 |
| `PORT` | | provided by Railway |

## 14. Deployment (Railway)

- **One service**, built from the Dockerfile: a Rust build stage plus a slim runtime image with the binary, `public/` and assets.
- **Volume:** one Railway volume mounted at `/data`.
- **Always-on:** Railway's Serverless/sleep setting stays **off**, since sleeping would break live updates and cold starts would slow down shares.
- **Instances:** exactly 1 replica.
- **Variables:** set from §13 via the Railway CLI or dashboard.
- **Health check:** `/healthz`.
- **Redeploys** cause a few seconds of downtime, because a volume can't be attached to two instances at once. Open pages reconnect on their own.

## 15. Milestones

### M0: feasibility spike ✅ done 2026-10-14

- Hello-world Topcoat app on Railway with a volume.
- Manifest plus a minimal `sw.js`. Install as a PWA on Android Chrome.
- Share text from another app to `/share` and see it on Linux **live** over SSE.
- Check the open questions in §16.

### M1: text and links ✅ done 2026-10-14

- Login, cookie, logout.
- Text and link drops, live list, Copy, Open, Delete, Clear all.
- Expiry: default plus override, cleanup task.

### M2: files ✅ done 2026-10-14

- Streaming uploads to `tmp/`, atomic rename, limits, download, inline image previews (no generated thumbnails yet; add them if large photos make the list slow).
- The upload queue sends **one request per text or file**, so each has its own progress, cancel and retry.
- `upload.js`: progress, paste, drag-and-drop.
- `sw.js` share intake with the confirm screen and retry.

### M3: comfort

- QR pairing.
- `once` expiry.
- API token plus curl examples.
- Optional "Large file? → open PairDrop" hint above `MAX_FILE_BYTES`.

## 16. Open questions to check in M0

1. ✅ **Answered in M0:** Android Chrome installs the PWA with the minimal `sw.js` (no fetch handler) and offers it in the Share menu. The share POST arrives **with** the `SameSite=Lax` login cookie. ✅ File sharing works with an explicit `accept` list (`image/*`, `application/*`, … plus extensions); after changing `share_target` the PWA has to be reinstalled before Android picks it up.
2. Railway's proxy: ✅ SSE works through it (long idle periods not yet observed). ✅ A 60 MB upload took 22 s and came back byte-identical, so the proxy doesn't block uploads within `MAX_FILE_BYTES`.
3. ✅ Topcoat 0.10 handles streaming multipart, signed cookies and `public/` at the root. Memory on Railway: about 3 MB idle, peaking under 6 MB while moving a 60 MB file.
4. Datastar under a strict Content-Security-Policy (it evaluates expressions, so it may need `'unsafe-eval'`).
5. Thumbnails: postponed. Previews show the original image; HEIC photos get no preview, just the file row.
