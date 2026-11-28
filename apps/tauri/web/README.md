# FileTerm Web (Docker)

Run FileTerm as a native self-hosted Web application in Docker. The image compiles the standalone Rust backend binary (`fileterm-web` with `--no-default-features --features web`) together with the existing React renderer (`apps/tauri/dist`), fronted by Caddy for authentication and optional TLS — with zero Xvfb, X11, or noVNC.

## Quick start on a Linux host or NAS (LAN access)

Docker Hub stores the image; it does not choose the container's network mode. Docker, Compose, or the NAS container manager makes that choice ([Compose `network_mode` reference](https://docs.docker.com/reference/compose-file/container-spec/#network_mode)). For direct LAN access on Linux or a NAS, use host networking and set `FILETERM_WEB_LISTEN=0.0.0.0`. The image defaults this listener to `127.0.0.1`, so host networking alone leaves FileTerm available only on the host itself.

This follows the Home Assistant Container pattern: use host networking and open the service at the host's IP and port. Home Assistant's Synology instructions use Host network mode and the NAS IP as well ([Linux guide](https://www.home-assistant.io/installation/linux), [Synology guide](https://www.home-assistant.io/installation/alternative/)). With host networking, do not add a port mapping.

```bash
docker volume create fileterm-data
docker run -d \
  --name fileterm-web \
  --restart unless-stopped \
  --network host \
  --env FILETERM_WEB_PORT=34538 \
  --env FILETERM_WEB_LISTEN=0.0.0.0 \
  --volume fileterm-data:/data \
  --cap-drop ALL \
  --security-opt no-new-privileges:true \
  st0ff3l/fileterm:v2.3.4-beta.15
```

The same image is also available as `ghcr.io/st0ff3l/fileterm:v2.3.4-beta.15`.

Open `http://<NAS-LAN-IP>:34538` (for example, `http://192.168.1.10:34538`). `0.0.0.0` listens on all NAS interfaces. In Synology Container Manager, choose **Host** for Network and set `FILETERM_WEB_LISTEN` to `0.0.0.0`. If DSM Firewall is enabled, allow TCP port `34538` from your LAN.

On a new data volume without deployment credentials, sign in with `admin` / `admin`; FileTerm requires you to change the username and password before opening the workspace. Keep the HTTP listener on a trusted network. For public access, use an HTTPS reverse proxy and configure a strong login before exposing it.

Keep the `fileterm-data` volume when upgrading or recreating the container. It stores FileTerm settings, profiles, session secrets, and the browser-managed login. Back it up before moving or removing it.

To set deployment credentials instead of using the first-login flow, add `--env-file ./fileterm.env` to `docker run` and create a private file with `FILETERM_WEB_USERNAME` and `FILETERM_WEB_PASSWORD`. Keep that file out of source control. Deployment credentials take precedence over the browser-managed account.

## Compose from a source checkout

Run from the repository root. Use a matching GitHub release tag for the image:

```bash
export FILETERM_WEB_IMAGE=st0ff3l/fileterm:v2.3.4-beta.15
docker compose -f apps/tauri/web/compose.yaml pull
docker compose -f apps/tauri/web/compose.yaml up -d --no-build
```

For GHCR, use `ghcr.io/st0ff3l/fileterm:v2.3.4-beta.15`. To build from the checkout instead, leave `FILETERM_WEB_IMAGE` unset and run `docker compose -f apps/tauri/web/compose.yaml up -d --build`.

Open `http://<host>:34538` from a device that can reach the Docker host. When you run this Compose file on the same machine, `http://localhost:34538` also works. The service uses host networking and listens on all host interfaces. For a new data volume with no deployment credentials, sign in with `admin` / `admin`. FileTerm opens **Settings → Web Login** and requires a new password before opening the workspace. You can change both the username and password there. FileTerm stores a password hash in `/data/.web-account.json`. Set `FILETERM_WEB_PORT` to use another port.

Keep first-time setup on a trusted network, or specify credentials at deployment. Deployment credentials take precedence over the stored account and are managed through Docker. Set `FILETERM_WEB_USERNAME` with a password to override the username. A deployment password also gives an administrator a way to recover access from the server.

```bash
FILETERM_WEB_PASSWORD='change-me-now' \
  docker compose -f apps/tauri/web/compose.yaml up -d --no-build
```

Or pass a pre-hashed bcrypt password:

```bash
HASH="$(docker run --rm caddy:2 caddy hash-password --plaintext 'change-me-now')"
FILETERM_WEB_PASSWORD_HASH="$HASH" \
  docker compose -f apps/tauri/web/compose.yaml up -d --no-build
```

Set `FILETERM_WEB_DISABLE_AUTH=true` only when an authenticated reverse proxy handles sign-in.

## Host networking and remote access

The quick-start command and source-checkout Compose file use host networking and listen on `0.0.0.0:34538`, making FileTerm available through the host's LAN IP. This is the same network pattern shown in Home Assistant's Linux Container instructions. Host networking shares the host network stack and reduces network isolation. No port mapping is used or needed; Docker rejects `ports` together with `network_mode: host`.

FileTerm's Docker image defaults `FILETERM_WEB_LISTEN` to `127.0.0.1`. Set it to `0.0.0.0` for LAN access, including when using the image from Docker Hub with Host network mode. The host network also lets FileTerm reach an SSH server on the Docker host through `127.0.0.1:22`. Docker Desktop requires its optional host networking support.

For local-only access through a host-local reverse proxy, use bridge networking and publish the port on `127.0.0.1`. In this mode, Caddy inside the container must listen on `0.0.0.0` for the Docker port mapping to reach it. To connect to an SSH server on the Docker host from a bridge container, use its reachable IP or `host.docker.internal` rather than `127.0.0.1`. On Linux, add the host-gateway alias if needed. The source-checkout Compose service can be changed to this bridge setup:

```yaml
services:
  fileterm:
    network_mode: bridge
    extra_hosts:
      - 'host.docker.internal:host-gateway'
    ports:
      - '127.0.0.1:34538:34538'
    environment:
      FILETERM_WEB_PORT: '34538'
      FILETERM_WEB_LISTEN: '0.0.0.0'
```

With this alias, use `host.docker.internal` as the SSH host name when connecting to the Docker host from FileTerm.

- **Internal TLS**: set `FILETERM_WEB_TLS=internal` and use HTTPS. Trust the generated Caddy CA on the clients.
- **Custom certificate**: pass readable PEM mounts to the container and set `FILETERM_WEB_TLS_CERT` and `FILETERM_WEB_TLS_KEY` in the service environment.
- **Host reverse proxy**: for proxy-only access, set `FILETERM_WEB_LISTEN=127.0.0.1` with host networking. Preserve the public Host, pass the external `X-Forwarded-Proto` and `X-Forwarded-Host`, and forward WebSocket connections to the loopback gateway. Caddy trusts loopback proxies when this listener is set to `127.0.0.1`.

Caddy enforces Basic Auth when credentials are supplied at deploy time. Otherwise, the Docker login form uses an HttpOnly session cookie to protect RPC, file transfers, and WebSocket upgrades. Changing browser-managed credentials revokes the previous cookie and closes its WebSockets. The Rust backend listens exclusively on a Unix socket inside a private directory and checks request origins. The unauthenticated `/healthz` endpoint returns service metadata only. The standalone development binary also supports a loopback HTTP listener without the Caddy authentication layer.

## Image tags and resource sizing

Use the exact release tag, such as `st0ff3l/fileterm:v2.3.4-beta.15`. It contains both `linux/amd64` and `linux/arm64`; Docker selects the matching platform automatically. The `-amd64` and `-arm64` tags are available for explicitly selecting one architecture and are not separate application versions.

The current beta release does not publish a `latest` tag. Always specify a version; an untagged `docker pull st0ff3l/fileterm` tries `latest` and will fail until that alias is published. Pre-releases should not move `latest`; reserve it for a stable release if you choose to offer that alias.

FileTerm does not yet publish a tested minimum CPU or memory requirement, and the Compose file does not set hard resource limits. Start with Docker's normal resource allocation, then monitor usage under the number of concurrent sessions you expect before adding limits.

## Architecture, data persistence, and upgrades

- **Native Web stack**: the React renderer detects the `Web` runtime via `/api/bootstrap`, routes RPC calls to `POST /api/rpc`, streams events and terminal output over `/api/ws`, renders native sub-windows via modal overlays (`WebWindowOverlayHost`), and transfers browser files through `/api/web/*`.
- **Container boundary**: local terminals, local file browsing, SSH agents, and network connections run inside the container as its configured user environment. Mount host directories or SSH keys explicitly if needed.
- **Persistent volume (`/data`)**: `fileterm-data` stores FileTerm profiles/settings, `/data/.machine-id` (for encrypted credential compatibility), `/data/.session-secret`, and the browser-managed Web account when configured. Keep the named volume across container rebuilds and image upgrades.

### Existing data volumes

The image runs as UID/GID `10001:10001`. New named volumes are initialized with that ownership. If your named volume was created by an older root-based image, stop FileTerm and back it up before changing its ownership. Replace `web_fileterm-data` with your actual named volume.

```bash
docker compose -f apps/tauri/web/compose.yaml stop
docker run --rm --mount type=volume,source=web_fileterm-data,target=/data \
  debian:bookworm-slim chown -hR 10001:10001 /data
```

Keep the existing `.machine-id` and credential files. For a host bind mount, give UID/GID `10001:10001` access to the required directory according to your host's permissions.

## Release automation

Pushing a validated `v*` release tag starts the desktop build and the Docker build in the same GitHub Actions run. The Docker job tests both Linux architectures before publishing a multi-platform image with the exact Git tag to Docker Hub and GHCR. Each pre-release keeps its own version tag.

The release job uses the GitHub Actions repository variable `DOCKERHUB_USERNAME` and the secret `DOCKERHUB_TOKEN` (a Docker Hub personal access token with Read & Write permission). The Docker Hub repository and GHCR package must be public for users to pull anonymously.

## Acceptance tests

With Docker, Python 3, `playwright` (`sync_playwright`), and Chromium/Chrome installed:

```bash
docker build -f apps/tauri/web/Dockerfile -t fileterm-web:local .
FILETERM_TEST_NETWORK=host FILETERM_TEST_TLS=1 bash apps/tauri/web/tests/run.sh fileterm-web:local
```

For a local bridge smoke test, omit `FILETERM_TEST_NETWORK=host`. On macOS with Chrome installed, pass `FILETERM_TEST_CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"`. The suite verifies HTTP Basic Auth, `HttpOnly` session cookies, same-origin checks, Playwright browser UI flows (creating a host profile, opening a local terminal, zero console/page errors), persistence across container restarts, and `FILETERM_WEB_TLS=internal`.
