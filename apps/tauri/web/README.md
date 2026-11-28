# FileTerm Web (Docker)

Run FileTerm as a native self-hosted Web application in Docker. The image compiles the standalone Rust backend binary (`fileterm-web` with `--no-default-features --features web`) together with the existing React renderer (`apps/tauri/dist`), fronted by Caddy for authentication and optional TLS — with zero Xvfb, X11, or noVNC.

## Quick start with Docker Compose

Run from the repository root:

```bash
docker compose -f apps/tauri/web/compose.yaml up -d --build
```

Open [http://localhost:34538](http://localhost:34538) and sign in with username `admin` and password `admin`. Set `FILETERM_WEB_PORT` to use another port.

Set a custom password before making FileTerm reachable from other devices. Set `FILETERM_WEB_USERNAME` to override the username.

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

Set `FILETERM_WEB_DISABLE_AUTH=true` only when an authenticated reverse proxy handles sign-in.

## Host networking and remote access

On Linux, the default Compose service shares the host network. FileTerm can connect to the host's SSH server at `127.0.0.1:22` if SSH is listening there. Caddy listens on `127.0.0.1:34538`; use an HTTPS reverse proxy on the same host for access from another device. Docker Desktop requires its optional host networking support. A bridge deployment can override the following service settings:

```yaml
services:
  fileterm:
    network_mode: bridge
    ports:
      - '127.0.0.1:34538:34538'
    environment:
      FILETERM_WEB_PORT: '34538'
      FILETERM_WEB_LISTEN: '0.0.0.0'
```

On bridge, connect to SSH using a reachable host IP or a `host-gateway` alias.

- **Internal TLS**: set `FILETERM_WEB_TLS=internal` and use HTTPS. Trust the generated Caddy CA on the clients.
- **Custom certificate**: pass readable PEM mounts to the container and set `FILETERM_WEB_TLS_CERT` and `FILETERM_WEB_TLS_KEY` in the service environment.
- **Host reverse proxy**: preserve the public Host, pass the external `X-Forwarded-Proto` and `X-Forwarded-Host`, and forward WebSocket connections to the loopback gateway. Caddy trusts loopback proxies for these headers in the default host configuration.

Caddy authenticates the page, HTTP RPC, file requests and WebSocket upgrades. The Rust backend in Docker listens exclusively on a Unix socket inside a private directory. It checks request origins and requires its session cookie for WebSocket connections. The unauthenticated `/healthz` endpoint returns service metadata only. The standalone development binary also supports a loopback HTTP listener without the Caddy authentication layer.

## Architecture, data persistence, and upgrades

- **Native Web stack**: the React renderer detects the `Web` runtime via `/api/bootstrap`, routes RPC calls to `POST /api/rpc`, streams events and terminal output over `/api/ws`, renders native sub-windows via modal overlays (`WebWindowOverlayHost`), and transfers browser files through `/api/web/*`.
- **Container boundary**: local terminals, local file browsing, SSH agents, and network connections run inside the container as its configured user environment. Mount host directories or SSH keys explicitly if needed.
- **Persistent volume (`/data`)**: `fileterm-data` stores FileTerm profiles/settings, `/data/.machine-id` (for encrypted credential compatibility), and `/data/.session-secret`. Keep the named volume across container rebuilds and image upgrades.

### Existing data volumes

The image runs as UID/GID `10001:10001`. New named volumes are initialized with that ownership. If your named volume was created by an older root-based image, stop FileTerm and back it up before changing its ownership. Replace `web_fileterm-data` with your actual named volume.

```bash
docker compose -f apps/tauri/web/compose.yaml stop
docker run --rm --mount type=volume,source=web_fileterm-data,target=/data \
  debian:bookworm-slim chown -hR 10001:10001 /data
```

Keep the existing `.machine-id` and credential files. For a host bind mount, give UID/GID `10001:10001` access to the required directory according to your host's permissions.

## Acceptance tests

With Docker, Python 3, `playwright` (`sync_playwright`), and Chromium/Chrome installed:

```bash
docker build -f apps/tauri/web/Dockerfile -t fileterm-web:local .
FILETERM_TEST_NETWORK=host FILETERM_TEST_TLS=1 bash apps/tauri/web/tests/run.sh fileterm-web:local
```

For a local bridge smoke test, omit `FILETERM_TEST_NETWORK=host`. On macOS with Chrome installed, pass `FILETERM_TEST_CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"`. The suite verifies HTTP Basic Auth, `HttpOnly` session cookies, same-origin checks, Playwright browser UI flows (creating a host profile, opening a local terminal, zero console/page errors), persistence across container restarts, and `FILETERM_WEB_TLS=internal`.
