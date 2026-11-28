#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
IMAGE="${1:-}"
PORT="${FILETERM_TEST_PORT:-18429}"
TLS_PORT="${FILETERM_TEST_TLS_PORT:-18430}"
VOLUME_NAME="fileterm-web-smoke-data-$$"
CONTAINER_NAME="fileterm-web-smoke-$$"
TLS_CONTAINER_NAME="fileterm-web-smoke-tls-$$"
PASSWORD="smoke-password-${RANDOM}"
BUILT_IMAGE=""

cleanup() {
  docker rm -f "$CONTAINER_NAME" "$TLS_CONTAINER_NAME" >/dev/null 2>&1 || true
  docker volume rm -f "$VOLUME_NAME" >/dev/null 2>&1 || true
  if [[ -n "$BUILT_IMAGE" ]]; then
    docker rmi -f "$BUILT_IMAGE" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if [[ -z "$IMAGE" ]]; then
  BUILT_IMAGE="fileterm-web:smoke-$$"
  IMAGE="$BUILT_IMAGE"
  echo "==> Building native FileTerm Web image ($IMAGE)..."
  docker build -f "$ROOT_DIR/apps/tauri/web/Dockerfile" -t "$IMAGE" "$ROOT_DIR"
fi

echo "==> Checking image contains zero X11 / Xvfb / noVNC binaries..."
if docker run --rm --entrypoint sh "$IMAGE" -c 'command -v Xvfb || command -v x11vnc || command -v fluxbox || command -v websockify' >/dev/null 2>&1; then
  echo "ERROR: Unexpected legacy X11/Xvfb/noVNC binaries found in $IMAGE" >&2
  exit 1
fi

docker volume create "$VOLUME_NAME" >/dev/null

echo "==> Starting container with Basic Auth enabled on http://127.0.0.1:$PORT..."
docker run -d \
  --name "$CONTAINER_NAME" \
  -p "127.0.0.1:${PORT}:8080" \
  -v "${VOLUME_NAME}:/data" \
  -e "FILETERM_WEB_USERNAME=tester" \
  -e "FILETERM_WEB_PASSWORD=$PASSWORD" \
  "$IMAGE" >/dev/null

for _ in $(seq 1 60); do
  if curl -fsS "http://127.0.0.1:${PORT}/healthz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done
curl -fsS "http://127.0.0.1:${PORT}/healthz" >/dev/null

echo "==> Running HTTP/WebSocket/Playwright UI smoke tests (pass 1)..."
python3 "$ROOT_DIR/apps/tauri/web/tests/smoke.py" \
  --base-url "http://127.0.0.1:${PORT}" \
  --username "tester" \
  --password "$PASSWORD" \
  --phase create-and-verify

echo "==> Restarting container against the same /data volume to verify persistence..."
docker rm -f "$CONTAINER_NAME" >/dev/null
docker run -d \
  --name "$CONTAINER_NAME" \
  -p "127.0.0.1:${PORT}:8080" \
  -v "${VOLUME_NAME}:/data" \
  -e "FILETERM_WEB_USERNAME=tester" \
  -e "FILETERM_WEB_PASSWORD=$PASSWORD" \
  "$IMAGE" >/dev/null

for _ in $(seq 1 60); do
  if curl -fsS "http://127.0.0.1:${PORT}/healthz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done

echo "==> Verifying persisted connection survives container restart (pass 2)..."
python3 "$ROOT_DIR/apps/tauri/web/tests/smoke.py" \
  --base-url "http://127.0.0.1:${PORT}" \
  --username "tester" \
  --password "$PASSWORD" \
  --phase verify-persisted

if [[ "${FILETERM_TEST_TLS:-0}" == "1" ]]; then
  echo "==> Verifying HTTPS (FILETERM_WEB_TLS=internal) mode..."
  docker rm -f "$CONTAINER_NAME" >/dev/null
  docker run -d \
    --name "$TLS_CONTAINER_NAME" \
    -p "127.0.0.1:${TLS_PORT}:8080" \
    -v "${VOLUME_NAME}:/data" \
    -e "FILETERM_WEB_USERNAME=tester" \
    -e "FILETERM_WEB_PASSWORD=$PASSWORD" \
    -e "FILETERM_WEB_TLS=internal" \
    "$IMAGE" >/dev/null

  for _ in $(seq 1 60); do
    if curl -kfsS "https://127.0.0.1:${TLS_PORT}/healthz" >/dev/null 2>&1; then
      break
    fi
    sleep 0.5
  done

  python3 "$ROOT_DIR/apps/tauri/web/tests/smoke.py" \
    --base-url "https://127.0.0.1:${TLS_PORT}" \
    --username "tester" \
    --password "$PASSWORD" \
    --insecure \
    --phase verify-persisted
fi

echo "==> All FileTerm Web smoke tests passed!"
