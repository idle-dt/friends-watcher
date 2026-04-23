# Friends Watcher

A single-user macOS desktop app that tracks one Instagram account's followers
and surfaces who unfollowed since the last sync. Not distributed via the App
Store — build and run locally.

![Screenshot placeholder](docs/screenshot.png)

## Overview

Friends Watcher logs into your own Instagram account inside an embedded
WKWebView, then uses the session cookies to read your followers and following
lists through Instagram's internal web API. Each sync is written to a local
SQLite snapshot; the diff between the two most recent snapshots tells you who
unfollowed you since last time.

v1 is strictly read-only:

- No unfollow actions.
- No auto-sync, no background jobs, no notifications.
- Every sync is user-initiated via the Sync button.

## Privacy

- No password ever leaves the webview. You log in to Instagram the same way you
  would in a browser; Friends Watcher never sees your credentials.
- No server. There is no backend — the app talks directly to Instagram
  (`instagram.com/api/v1/` for data, Instagram's image CDN for avatars) from
  your machine.
- No telemetry. Nothing is sent anywhere except Instagram.
- All data — session cookies, follower snapshots, and cached profile avatars
  — lives in `~/Library/Application Support/com.friendswatcher.app/`. Delete
  that folder to wipe everything.

## Prerequisites

- macOS 10.15 or newer.
- [Rust stable](https://www.rust-lang.org/tools/install) (install via
  `rustup`).
- Node.js 20 or newer.
- Xcode Command Line Tools: `xcode-select --install`.

## Dev loop

```sh
npm install
npm run tauri dev
```

`tauri dev` starts the Vite dev server and a debug build of the Tauri shell.
First launch takes a while because Cargo has to compile the Rust dependencies;
subsequent launches are fast.

If you prefer invoking the Tauri CLI directly:

```sh
cargo tauri dev
```

(requires `cargo install tauri-cli --version '^2'`; the repo's npm-scripts
route is the recommended path since it pins the CLI version via
`@tauri-apps/cli`.)

## Release build

Universal (Intel + Apple Silicon) `.app` + `.dmg`:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run tauri build -- --target universal-apple-darwin
```

Output lands in `src-tauri/target/universal-apple-darwin/release/bundle/`.

## First launch

The bundle is unsigned, so Gatekeeper will block it on a fresh Mac. To open it
the first time:

1. In Finder, right-click (or Control-click) `Friends Watcher.app` and choose
   **Open**.
2. macOS will warn that the app is from an unidentified developer. Click
   **Open** again.
3. Log in with your Instagram account inside the embedded window. The app
   detects the login by watching the `sessionid` cookie and flips to the main
   view automatically.
4. Click **Sync**. The first sync takes ~1.5 seconds per 50 followers (plus
   the same for followings) because of the built-in rate-limit pacing.

Subsequent launches open normally.

## Change markers

After each sync, rows in the main list are tagged relative to the previous
snapshot:

- New followers get a green stripe on the left edge and a **New** pill.
- People who unfollowed you (and whom you still follow) get an amber stripe
  and an **Unfollowed you** pill.

The diff banner at the top still shows the aggregate counts; the per-row
markers tell you which rows those counts refer to.

## Troubleshooting

**"Please log in again" banner.** Instagram returned 401 or a
`login_required` body. Click the banner's login button; the app reopens the
Instagram login page in the embedded webview. Re-authenticate and retry the
sync.

**"Instagram is rate-limiting — try again later" banner.** Instagram returned
429 or a `feedback_required` / `checkpoint_required` body. The client already
retried with 5s / 15s / 45s backoff before surfacing the banner. Wait a few
minutes (sometimes hours, if you hit a checkpoint) and retry. If you keep
hitting the banner, log in to Instagram in a normal browser and clear any
security checkpoints there first.

**Sync stops partway.** The API client has a hard cap of 20,000 users per
sync as a defensive safeguard. If you legitimately have more, that cap needs
to be lifted in `src-tauri/src/instagram.rs`.

**Avatar looks stale.** Avatars are fetched through the Rust backend and
cached on disk under
`~/Library/Application Support/com.friendswatcher.app/avatars/<ig_user_id>`.
The cache has no expiry — if someone changes their profile picture, delete
that file (or the whole `avatars/` folder) and sync again to refresh.

**Starting over / switching accounts.** The Instagram session (cookies)
persists across launches in macOS's WebKit data directory. To start from a
clean state — useful for testing the first-launch flow or for switching to a
different Instagram account — close the app, then run:

```sh
rm -rf ~/Library/WebKit/com.friendswatcher.app
```

The next launch will show the Instagram login page from scratch.

## Known limits

- One Instagram account at a time.
- 20,000-user defensive cap per sync.
- v1 is read-only — no unfollow, no bulk actions.
- Sync is manual only. There is no scheduled or background refresh.
- Cached avatars never expire. If someone changes their profile picture, delete
  their file under `<app-data>/avatars/` to force a refetch.
- macOS only. The code leans on WKWebView and the macOS data directory layout.

## Recent changes

- Avatar cache writes now remove the `.tmp` sidecar when the atomic rename
  fails, instead of leaving it to linger in the cache directory.
- `RelationshipRow` revokes the avatar blob URL immediately on image-decode
  failure, shortening the window in which a broken object URL is retained.
- Sync fetches followers and following concurrently, roughly halving
  wall-clock time on accounts where both lists are sizeable.
- Cached avatars are downscaled to 64×64 JPEG before hitting disk, so each
  row loads a few kilobytes instead of the full CDN payload.
