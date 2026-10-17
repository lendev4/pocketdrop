# Deploy and Host PocketDrop with Railway

PocketDrop is a private drop zone for moving text, links and files between your own devices. Share something from any Android app and it appears instantly on your laptop, or paste and drag things into the page on your laptop and pick them up on your phone. Drops disappear on their own after first use or a time you choose.

## About Hosting PocketDrop

PocketDrop is a single small Rust binary that serves the web app, keeps a live connection to every open page and stores drops as plain files on a volume. This template builds it from the Dockerfile, attaches a volume at `/data`, adds a health check and generates the passphrase, session secret and API token for you. After the deploy, open the service's **Variables** tab to read your `PASSPHRASE`, then open the public URL and log in. On Android, install the site as an app from Chrome so PocketDrop shows up in the share menu.

## Common Use Cases

- Sending a link, a snippet or a photo from your phone to your desktop without messaging yourself
- Getting a file from your laptop onto your phone in a few seconds
- Sharing one-off text such as Wi-Fi passwords or codes between your devices, deleted after the first copy
- Sending files from a terminal or script with `curl`

## Dependencies for PocketDrop Hosting

- A Railway volume mounted at `/data` for the drops (included in this template)
- HTTPS, which Railway's public domain provides; the app needs it for login cookies, installing on Android and clipboard access

### Deployment Dependencies

- Source code: https://github.com/lendev4/pocketdrop
- Web framework: [Topcoat](https://github.com/tokio-rs/topcoat), with [Datastar](https://data-star.dev) for live updates
- Optional large-file transfers: [PairDrop](https://pairdrop.net)

### Implementation Details

The template sets these variables:

| Variable | Value |
|---|---|
| `PASSPHRASE` | Generated; change it in Variables if you like |
| `SESSION_SECRET` | Generated; changing it logs out every device |
| `API_TOKEN` | Generated; enables the `curl` API |
| `DEFAULT_EXPIRY` | `once`: drops are deleted after their first download or copy |

Limits such as `MAX_FILE_BYTES` (100 MB by default) and `MAX_TOTAL_BYTES` (2 GB) can be added as variables. See the [README](https://github.com/lendev4/pocketdrop#configuration) for every setting.

## Why Deploy PocketDrop on Railway?

Railway is a singular platform to deploy your infrastructure stack. Railway will host your infrastructure so you don't have to deal with configuration, while allowing you to vertically and horizontally scale it.

By deploying PocketDrop on Railway, you are one step closer to supporting a complete full-stack application with minimal burden. Host your servers, databases, AI agents, and more on Railway.
