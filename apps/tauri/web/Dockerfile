FROM node:22-bookworm-slim AS frontend-builder

WORKDIR /src

COPY package.json package-lock.json tsconfig.base.json THIRD_PARTY_NOTICES.md THIRD-PARTY-NOTICES.md ./
COPY docs/quality/iterm2-color-scheme-attribution.md ./docs/quality/
COPY scripts ./scripts
COPY packages ./packages
COPY apps/tauri/package.json apps/tauri/tsconfig.json apps/tauri/index.html apps/tauri/vite.config.ts ./apps/tauri/
COPY apps/tauri/public ./apps/tauri/public
COPY apps/tauri/scripts ./apps/tauri/scripts
COPY apps/tauri/src ./apps/tauri/src
COPY apps/tauri/src-tauri/icons ./apps/tauri/src-tauri/icons

RUN HUSKY=0 npm ci --ignore-scripts \
  && npm run build:packages \
  && npm --workspace @fileterm/tauri run sync:completion-specs \
  && npm --workspace @fileterm/tauri run build:renderer

FROM rust:1-bookworm AS rust-builder

WORKDIR /src

RUN apt-get update \
  && apt-get install -y --no-install-recommends pkg-config libudev-dev libssl-dev cmake \
  && rm -rf /var/lib/apt/lists/*

COPY vendor ./vendor
COPY apps/tauri/src-tauri/Cargo.toml apps/tauri/src-tauri/Cargo.lock apps/tauri/src-tauri/build.rs ./apps/tauri/src-tauri/
COPY apps/tauri/src-tauri/.cargo ./apps/tauri/src-tauri/.cargo
COPY apps/tauri/src-tauri/icons ./apps/tauri/src-tauri/icons
COPY apps/tauri/src-tauri/linux ./apps/tauri/src-tauri/linux
COPY apps/tauri/src/renderer/assets/app-icons ./apps/tauri/src/renderer/assets/app-icons
COPY apps/tauri/src-tauri/src ./apps/tauri/src-tauri/src

RUN cargo build \
  --manifest-path apps/tauri/src-tauri/Cargo.toml \
  --locked \
  --release \
  --no-default-features \
  --features web \
  --bin fileterm-web

FROM caddy:2 AS caddy-source

FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive \
  HOME=/data \
  XDG_DATA_HOME=/data/.local/share \
  XDG_CONFIG_HOME=/data/.config \
  FILETERM_WEB_DIST_DIR=/opt/fileterm/dist \
  FILETERM_WEB_PORT=34538 \
  FILETERM_WEB_LISTEN=127.0.0.1 \
  FILETERM_WEB_USERNAME=fileterm \
  FILETERM_WEB_TLS=off

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    libudev1 \
    openssh-client \
    tini \
  && rm -rf /var/lib/apt/lists/* \
  && mkdir -p /opt/fileterm/dist /data/.local/share /data/.config /etc/fileterm \
  && rm -f /etc/machine-id \
  && ln -s /data/.machine-id /etc/machine-id \
  && groupadd -g 10001 fileterm \
  && useradd -u 10001 -g 10001 -d /data -M fileterm \
  && chown -R 10001:10001 /data \
  && chmod 0700 /data

COPY --from=caddy-source /usr/bin/caddy /usr/local/bin/caddy
COPY --from=frontend-builder /src/apps/tauri/dist /opt/fileterm/dist
COPY --from=rust-builder /src/apps/tauri/src-tauri/target/release/fileterm-web /usr/local/bin/fileterm-web
COPY apps/tauri/web/entrypoint.sh /opt/fileterm/entrypoint.sh

RUN chmod +x /usr/local/bin/fileterm-web /usr/local/bin/caddy /opt/fileterm/entrypoint.sh

USER 10001:10001
EXPOSE 34538
VOLUME ["/data"]

HEALTHCHECK --interval=15s --timeout=5s --start-period=15s --retries=6 \
  CMD curl -kfsS "http://127.0.0.1:${FILETERM_WEB_PORT}/healthz" >/dev/null || \
      curl -kfsS "https://127.0.0.1:${FILETERM_WEB_PORT}/healthz" >/dev/null || exit 1

ENTRYPOINT ["/usr/bin/tini", "--", "/opt/fileterm/entrypoint.sh"]
