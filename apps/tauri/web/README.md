# FileTerm Web (Docker)

Run FileTerm as a native self-hosted Web application in Docker. The image compiles the standalone Rust backend binary (`fileterm-web` with `--no-default-features --features web`) together with the existing React renderer (`apps/tauri/dist`), fronted by Caddy for authentication and optional TLS — with zero Xvfb, X11, or noVNC.

## Quick start with Docker Compose

Run from the repository root with either a plaintext password or a bcrypt hash produced by `caddy hash-password`:

```bash
FILETERM_WEB_PASSWORD='change-me-now' \
  docker compose -f apps/tauri/web/compose.yaml up -d --build
```

Or pass a pre-hashed bcrypt password:

```bash
HASH="$(docker run --rm caddy:2 caddy hash-password --plaintext 'change-me-now')"
FILETERM_WEB_PASSWORD_HASH="$HASH" \
  docker compose -f apps/tauri/web/compose.yaml up -d --build
```

Open [http://localhost:8080](http://localhost:8080) on the host (override with `FILETERM_WEB_PORT`). Sign in as `fileterm` (override with `FILETERM_WEB_USERNAME`) and the password you configured.

The container refuses to start unless `FILETERM_WEB_PASSWORD` or `FILETERM_WEB_PASSWORD_HASH` is set (or `FILETERM_WEB_DISABLE_AUTH=true` when placed behind an authenticated reverse proxy).

## HTTPS and remote access

`compose.yaml` binds the published port to `127.0.0.1` by default. For LAN or remote access, enable TLS in Caddy or place an HTTPS reverse proxy in front of `127.0.0.1:8080`:

- **Self-signed internal TLS**: set `FILETERM_WEB_TLS=internal`.
- **Custom certificate**: mount PEM files and set `FILETERM_WEB_TLS_CERT` and `FILETERM_WEB_TLS_KEY`.
- **External reverse proxy**: forward `Host`, `X-Forwarded-Proto`, and `X-Forwarded-Host` so `fileterm-web` validates same-origin `Origin` headers on state-changing HTTP RPC calls and WebSocket upgrades, and issues an `HttpOnly` `fileterm_session` cookie (marked `Secure` on HTTPS requests).

## Architecture, data persistence, and upgrades

- **Native Web stack**: the React renderer detects the `Web` runtime via `/api/bootstrap`, routes RPC calls to `POST /api/rpc`, streams events and terminal output over `/api/ws`, renders native sub-windows via modal overlays (`WebWindowOverlayHost`), and transfers browser files through `/api/web/*`.
- **Container boundary**: local terminals, local file browsing, SSH agents, and network connections run inside the container as its configured user environment. Mount host directories or SSH keys explicitly if needed.
- **Persistent volume (`/data`)**: `fileterm-data` stores FileTerm profiles/settings, `/data/.machine-id` (for encrypted credential compatibility), and `/data/.session-secret`. Keep the named volume across container rebuilds and image upgrades.

## Acceptance tests

With Docker, Python 3, `playwright` (`sync_playwright`), and Chromium/Chrome installed:

```bash
docker build -f apps/tauri/web/Dockerfile -t fileterm-web:local .
FILETERM_TEST_TLS=1 bash apps/tauri/web/tests/run.sh fileterm-web:local
```

On macOS with Chrome installed, pass `FILETERM_TEST_CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"`. The suite verifies HTTP Basic Auth, `HttpOnly` session cookies, same-origin checks, Playwright browser UI flows (creating a host profile, opening a local terminal, zero console/page errors), persistence across container restarts, and `FILETERM_WEB_TLS=internal`.
