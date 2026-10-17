# PocketDrop

A private drop zone for moving text, links and files between your own devices, mainly from Android to a desktop and back.

Share something from any Android app and it shows up on your laptop immediately. Or paste or drag something into the page on your laptop and pick it up on your phone. Drops disappear on their own: after the first download or copy, or after a time you choose.

[![Deploy on Railway](https://railway.com/button.svg)](https://railway.com/deploy/pocketdrop)

## Features

- **Android share menu:** installed as an app (PWA), PocketDrop appears in Android's share menu for text, links, photos and files. No native app.
- **Live:** every open page updates instantly when something arrives, over server-sent events.
- **Files:** images, PDFs and documents up to 100 MB (configurable), with upload progress, retry, drag-and-drop and pasting screenshots with Ctrl+V.
- **Expiry per drop:** until first use, 5 min to 7 days, or until deleted. Each device remembers its choice.
- **QR pairing:** log in a new device by scanning a code instead of typing the passphrase.
- **Terminal API:** send and fetch drops with `curl`.
- **Large files:** above the limit, PocketDrop hands the file to [PairDrop](https://pairdrop.net) for a direct transfer and notifies your other devices.
- **Light:** a single Rust binary, about 3 MB of memory, plain files on disk, no database.

## Deploy on Railway

1. Click **Deploy on Railway** above. Railway builds the app, attaches a volume for the drops and generates the secrets.
2. When the deploy is done, open the service → **Variables** and copy **`PASSPHRASE`**. You can change it to something you like; the service then redeploys.
3. Open the service's public URL (under **Settings → Networking**, generate a domain if there is none) and log in with the passphrase.

The Railway Hobby plan's included usage is plenty: the app idles at a few MB of memory.

## Using it

**On Android:** open the site in Chrome, log in, then **⋮ → Install app** (or *Add to Home screen*). After that, **Share → PocketDrop** works from any app. Android stores the app's name, icon, colours and share settings at install time. After changing them, open `chrome://webapks` in Chrome and press **Update** for PocketDrop, or uninstall the app fully (long-press → **App info** → **Uninstall**; removing the home-screen icon is not enough) and install it again.

**Adding more devices:** on a logged-in device, open **⋯ → Pair a device** and scan the QR code with the other device's camera. The link works once, for 5 minutes.

**On the desktop:** keep the page open in a tab. Type or paste text, press **Ctrl+V** anywhere to send the clipboard (text or a screenshot), or drag files onto the page.

**Large files:** files over the limit get a **Send with PairDrop** button. Open <https://pairdrop.net> once in the browser on each device and pair them there (one time); afterwards PocketDrop opens PairDrop with the file ready to send, and your other devices get an *Incoming via PairDrop* notice. This uses undocumented PairDrop behaviour and may break if PairDrop changes.

## Terminal API

Set `API_TOKEN` (the template generates one) and use it as a bearer token:

```sh
export PD=https://your-app.up.railway.app PD_TOKEN=...

curl -H "Authorization: Bearer $PD_TOKEN" -F files=@report.pdf $PD/api/drops       # send a file
curl -H "Authorization: Bearer $PD_TOKEN" -F text="hello" -F expiry=10m $PD/api/drops
echo "piped" | curl -H "Authorization: Bearer $PD_TOKEN" -F 'text=<-' $PD/api/drops  # send stdin
curl -H "Authorization: Bearer $PD_TOKEN" $PD/api/latest                           # print the newest text
curl -H "Authorization: Bearer $PD_TOKEN" -OJ $PD/api/latest                       # save the newest file
```

Without `API_TOKEN`, `/api/*` does not exist.

## Configuration

All settings are environment variables.

| Variable | Default | Meaning |
|---|---|---|
| `PASSPHRASE` | (required) | The login passphrase |
| `SESSION_SECRET` | (required) | At least 64 random bytes; signs login cookies. Changing it logs out every device. |
| `SESSION_DAYS` | `365` | How long a device stays logged in |
| `API_TOKEN` | unset | Enables the terminal API |
| `DATA_DIR` | `data` (`/data` in Docker) | Where drops are stored |
| `DEFAULT_EXPIRY` | `24h` (`once` in the template) | Preselected expiry for new devices |
| `EXPIRY_OPTIONS` | `once,5m,10m,1h,24h,7d,never` | Choices offered when sending |
| `MAX_FILE_BYTES` | `104857600` (100 MB) | Largest file accepted |
| `MAX_TEXT_BYTES` | `1048576` (1 MB) | Largest text accepted |
| `MAX_TOTAL_BYTES` | `2147483648` (2 GB) | Cap on everything stored |
| `MAX_DROPS` | `50` | Above this, the oldest drops go (except "until deleted") |
| `PAIRDROP_URL` | `https://pairdrop.net/` | Direct-transfer service for large files |
| `TRUSTED_PROXY_HOPS` | `1` | Reverse proxies in front of the app (Railway: 1). Used to find the client's address for the login limit; set `0` when exposed directly. |
| `HOST`, `PORT` | `127.0.0.1`, `3000` | Bind address (`0.0.0.0` in Docker; Railway sets `PORT`) |

## Self-hosting with Docker

```sh
docker build -t pocketdrop .
docker run -d -p 3000:3000 -v pocketdrop-data:/data \
  -e PASSPHRASE='choose-something-long' \
  -e SESSION_SECRET="$(openssl rand -hex 64)" \
  pocketdrop
```

Put it behind HTTPS (for example Caddy or a Cloudflare Tunnel): login cookies are `Secure`, and browsers only install the app and allow clipboard access over HTTPS.

## Development

Tools are pinned with [mise](https://mise.jdx.dev):

```sh
mise install                         # Rust and the Topcoat CLI
PASSPHRASE=dev mise exec -- topcoat dev
```

Then open <http://127.0.0.1:3000>. `cargo test` runs the unit tests; `python3 scripts/icons.py` regenerates the app icons.

**How it works:** [Topcoat](https://github.com/tokio-rs/topcoat) renders the pages on the server, and [Datastar](https://data-star.dev) swaps in the updated list sent over server-sent events. Each drop is a folder with its content and a `meta.json`. A small service worker receives Android shares and keeps them on the phone until the upload succeeds. [`SPEC.md`](SPEC.md) has the full design and the decisions behind it.

**Note:** Topcoat is early-stage and pinned to an exact version; upgrade it deliberately.

## Security

- Single user: one passphrase, signed long-lived cookies, an `Origin` check on every change, and a constant-time passphrase comparison.
- After 5 wrong passphrases or API tokens from one address within a minute, that address has to wait a minute.
- Uploaded files are always served as downloads with `nosniff` and a sandboxing Content-Security-Policy; only raster images are shown inline.
- Content is protected by HTTPS and the login, and stored unencrypted on the server. It is not end-to-end encrypted.

## License

[MIT](LICENSE)
