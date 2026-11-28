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
  if [[ "$?" != 0 ]]; then
    docker logs "$CONTAINER_NAME" || true
    docker logs "$TLS_CONTAINER_NAME" || true
  fi
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

network_options() {
  local port="$1"
  if [[ "${FILETERM_TEST_NETWORK:-bridge}" == "host" ]]; then
    network_args=(--network host -e "FILETERM_WEB_PORT=$port")
  else
    network_args=(-p "127.0.0.1:$port:34538" -e FILETERM_WEB_LISTEN=0.0.0.0)
  fi
}
network_options "$PORT"

echo "==> Starting container with Basic Auth enabled on http://127.0.0.1:$PORT..."
docker run -d \
  --name "$CONTAINER_NAME" --cap-drop ALL --security-opt no-new-privileges \
  "${network_args[@]}" \
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

[[ "$(docker exec "$CONTAINER_NAME" id -u)" == "10001" ]]
docker exec "$CONTAINER_NAME" sh -ec '
  socket="$(find /tmp -type s -name backend.sock)"
  test -S "$socket"
  test "$(stat -c %a "$(dirname "$socket")")" = 700
  curl --unix-socket "$socket" -fsS http://localhost/healthz >/dev/null
  ! curl -fsS --max-time 2 http://127.0.0.1:18420/healthz >/dev/null 2>&1
'

echo "==> Running HTTP/WebSocket/Playwright UI smoke tests (pass 1)..."
python3 "$ROOT_DIR/apps/tauri/web/tests/smoke.py" \
  --base-url "http://127.0.0.1:${PORT}" \
  --username "tester" \
  --password "$PASSWORD" \
  --phase create-and-verify

echo "==> Checking migration from a root-owned volume and verifying persistence..."
machine_id="$(docker exec "$CONTAINER_NAME" cat /data/.machine-id)"
docker stop "$CONTAINER_NAME" >/dev/null
docker rm "$CONTAINER_NAME" >/dev/null
docker run --rm --user 0:0 --entrypoint chown -v "${VOLUME_NAME}:/data" "$IMAGE" -hR 0:0 /data
docker run --rm --user 0:0 --entrypoint chown -v "${VOLUME_NAME}:/data" "$IMAGE" -hR 10001:10001 /data
docker run -d \
  --name "$CONTAINER_NAME" --cap-drop ALL --security-opt no-new-privileges \
  "${network_args[@]}" \
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

test "$(docker exec "$CONTAINER_NAME" cat /data/.machine-id)" = "$machine_id"
echo "==> Verifying persisted connection survives container restart (pass 2)..."
python3 "$ROOT_DIR/apps/tauri/web/tests/smoke.py" \
  --base-url "http://127.0.0.1:${PORT}" \
  --username "tester" \
  --password "$PASSWORD" \
  --phase verify-persisted

if [[ "${FILETERM_TEST_TLS:-0}" == "1" ]]; then
  echo "==> Verifying HTTPS (FILETERM_WEB_TLS=internal) mode..."
  docker rm -f "$CONTAINER_NAME" >/dev/null
  network_options "$TLS_PORT"
  docker run -d \
    --name "$TLS_CONTAINER_NAME" --cap-drop ALL --security-opt no-new-privileges \
    "${network_args[@]}" \
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
