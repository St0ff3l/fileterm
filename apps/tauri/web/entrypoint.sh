#!/usr/bin/env bash
set -euo pipefail

export HOME="${HOME:-/data}"
export XDG_DATA_HOME="${XDG_DATA_HOME:-/data/.local/share}"
export XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-/data/.config}"
export FILETERM_WEB_DIST_DIR="${FILETERM_WEB_DIST_DIR:-/opt/fileterm/dist}"

WEB_PORT="${FILETERM_WEB_PORT:-34538}"
WEB_LISTEN="${FILETERM_WEB_LISTEN:-127.0.0.1}"
WEB_USER="${FILETERM_WEB_USERNAME:-admin}"
WEB_PASSWORD="${FILETERM_WEB_PASSWORD:-}"
WEB_PASSWORD_HASH="${FILETERM_WEB_PASSWORD_HASH:-}"
DISABLE_AUTH="${FILETERM_WEB_DISABLE_AUTH:-false}"
TLS_MODE="${FILETERM_WEB_TLS:-off}"
TLS_CERT="${FILETERM_WEB_TLS_CERT:-}"
TLS_KEY="${FILETERM_WEB_TLS_KEY:-}"

if ! [[ "$WEB_PORT" =~ ^[1-9][0-9]{0,4}$ ]] || (( WEB_PORT < 1 || WEB_PORT > 65535 )); then
  echo "ERROR: FILETERM_WEB_PORT must be between 1 and 65535 (got '$WEB_PORT')." >&2
  exit 2
fi

if [[ -n "$WEB_PASSWORD_HASH" && "$WEB_PASSWORD_HASH" != '$2'* ]]; then
  echo "ERROR: FILETERM_WEB_PASSWORD_HASH must be a bcrypt hash produced by 'caddy hash-password'." >&2
  exit 2
fi

if [[ ! -w /data ]]; then
  echo 'ERROR: /data must be writable by FileTerm UID 10001. See the existing-volume migration in the Docker README.' >&2
  exit 2
fi
umask 077
mkdir -p /data /data/.local/share /data/.config /data/.local/share/com.fileterm.desktop
chmod 0700 /data || true

if [[ ! -s /data/.machine-id ]]; then
  if [[ -r /proc/sys/kernel/random/uuid ]]; then
    tr -d '-' </proc/sys/kernel/random/uuid >/data/.machine-id
  else
    date +%s%N | sha256sum | cut -c1-32 >/data/.machine-id
  fi
  chmod 0444 /data/.machine-id || true
fi

if [[ -z "${FILETERM_WEB_SESSION_SECRET:-}" ]]; then
  SESSION_SECRET_FILE="/data/.session-secret"
  if [[ ! -s "$SESSION_SECRET_FILE" ]]; then
    tr -d '\n' </proc/sys/kernel/random/uuid >"$SESSION_SECRET_FILE"
    chmod 0600 "$SESSION_SECRET_FILE" || true
  fi
  export FILETERM_WEB_SESSION_SECRET="$(cat "$SESSION_SECRET_FILE")"
fi

export FILETERM_WEB_DOCKER_LOGIN=1
unset FILETERM_WEB_ACCOUNT_FILE
if [[ "$DISABLE_AUTH" == "true" ]]; then
  CADDY_AUTH_BLOCK=""
elif [[ -z "$WEB_PASSWORD" && -z "$WEB_PASSWORD_HASH" ]]; then
  export FILETERM_WEB_ACCOUNT_FILE=/data/.web-account.json
  CADDY_AUTH_BLOCK=""
else
  if [[ -z "$WEB_PASSWORD_HASH" ]]; then
    WEB_PASSWORD_HASH="$(caddy hash-password --plaintext "$WEB_PASSWORD")"
  fi
  CADDY_AUTH_BLOCK=$'\t\tbasic_auth {\n\t\t\t'"$WEB_USER"$' '"$WEB_PASSWORD_HASH"$'\n\t\t}'
fi

CADDY_GLOBAL_EXTRA=""
if [[ -n "$TLS_CERT" || -n "$TLS_KEY" ]]; then
  if [[ -z "$TLS_CERT" || -z "$TLS_KEY" || ! -r "$TLS_CERT" || ! -r "$TLS_KEY" ]]; then
    echo "ERROR: Both FILETERM_WEB_TLS_CERT and FILETERM_WEB_TLS_KEY must point to readable PEM files." >&2
    exit 2
  fi
  CADDY_GLOBAL_EXTRA=$'\tauto_https disable_redirects'
  CADDY_SITE_ADDRESS="https://:${WEB_PORT}"
  CADDY_TLS_LINE=$'\ttls '"\"${TLS_CERT}\" \"${TLS_KEY}\""
elif [[ "$TLS_MODE" == "internal" ]]; then
  CADDY_GLOBAL_EXTRA=$'\tauto_https disable_redirects\n\tdefault_sni localhost'
  CADDY_SITE_ADDRESS="https://:${WEB_PORT}"
  CADDY_TLS_LINE=$'\ttls internal {\n\t\ton_demand\n\t}'
elif [[ "$TLS_MODE" == "off" ]]; then
  CADDY_SITE_ADDRESS="http://:${WEB_PORT}"
  CADDY_TLS_LINE=""
else
  echo "ERROR: Unsupported FILETERM_WEB_TLS mode '$TLS_MODE' (expected off, internal, or cert+key paths)." >&2
  exit 2
fi

unset WEB_PASSWORD FILETERM_WEB_PASSWORD

RUNTIME_DIR="$(mktemp -d)"
export FILETERM_WEB_SOCKET="$RUNTIME_DIR/backend.sock"
CADDYFILE_PATH="$RUNTIME_DIR/Caddyfile"
(
  umask 077
  {
    printf '{\n\tadmin off\n\tpersist_config off\n'
    if [[ "$WEB_LISTEN" == "127.0.0.1" ]]; then
      printf '\tservers {\n\t\ttrusted_proxies static 127.0.0.1/32 ::1/128\n\t}\n'
    fi
    if [[ -n "$CADDY_GLOBAL_EXTRA" ]]; then
      printf '%s\n' "$CADDY_GLOBAL_EXTRA"
    fi
    printf '}\n\n'
    printf '%s {\n' "$CADDY_SITE_ADDRESS"
    printf '\tbind %s\n' "$WEB_LISTEN"
    if [[ -n "$CADDY_TLS_LINE" ]]; then
      printf '%s\n' "$CADDY_TLS_LINE"
    fi
    cat <<EOF
	@health path /healthz
	handle @health {
		reverse_proxy unix/$FILETERM_WEB_SOCKET {
			header_up -Authorization
		}
	}

	handle {
EOF
    if [[ -n "$CADDY_AUTH_BLOCK" ]]; then
      printf '%s\n' "$CADDY_AUTH_BLOCK"
    fi
    cat <<EOF
		reverse_proxy unix/$FILETERM_WEB_SOCKET {
			header_up -Authorization
		}
	}
}
EOF
  } >"$CADDYFILE_PATH"
)
chmod 0600 "$CADDYFILE_PATH"
caddy validate --config "$CADDYFILE_PATH" --adapter caddyfile >/dev/null

/usr/local/bin/fileterm-web &
BACKEND_PID=$!

CADDY_PID=""
cleanup() {
  local exit_code=$?
  trap - SIGTERM SIGINT EXIT
  if [[ -n "$CADDY_PID" ]] && kill -0 "$CADDY_PID" 2>/dev/null; then
    kill -TERM "$CADDY_PID" 2>/dev/null || true
    wait "$CADDY_PID" 2>/dev/null || true
  fi
  if kill -0 "$BACKEND_PID" 2>/dev/null; then
    kill -TERM "$BACKEND_PID" 2>/dev/null || true
    wait "$BACKEND_PID" 2>/dev/null || true
  fi
  rm -f -- "$CADDYFILE_PATH" "$FILETERM_WEB_SOCKET"
  rmdir -- "$RUNTIME_DIR"
  exit "$exit_code"
}
trap cleanup SIGTERM SIGINT EXIT

ready=0
for _ in $(seq 1 60); do
  if curl --unix-socket "$FILETERM_WEB_SOCKET" -fsS http://localhost/healthz >/dev/null 2>&1; then
    ready=1
    break
  fi
  if ! kill -0 "$BACKEND_PID" 2>/dev/null; then
    echo "ERROR: fileterm-web exited before becoming ready." >&2
    exit 1
  fi
  sleep 0.25
done

if [[ "$ready" -ne 1 ]]; then
  echo "ERROR: fileterm-web socket health check timed out." >&2
  exit 1
fi

caddy run --config "$CADDYFILE_PATH" --adapter caddyfile &
CADDY_PID=$!

set +e
wait -n "$BACKEND_PID" "$CADDY_PID"
status=$?
set -e
exit "$status"
