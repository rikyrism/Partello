#!/usr/bin/env bash
# Share Partello from this computer through a free Cloudflare quick tunnel.
#
#   ./scripts/tunnel.sh            # build (first time), start everything
#   Ctrl+C                          # stop the tunnel and the server
#
# Friends open the https://….trycloudflare.com address it prints. The address
# changes every run, and this computer must stay on while people watch.
#
# Optional settings (free TURN so calls work on mobile data, LiveKit Cloud,
# YouTube search) go in .env.tunnel; see .env.tunnel.example.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT="${PORT:-8080}"
PG_PORT="${POSTGRES_PORT:-5433}"
LOG_DIR="$ROOT/.tunnel"
mkdir -p "$LOG_DIR"

export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"

if [ -f "$ROOT/.env.tunnel" ]; then
  set -a
  # shellcheck disable=SC1091
  . "$ROOT/.env.tunnel"
  set +a
  echo "› Settings from .env.tunnel loaded"
fi

for cmd in cloudflared cargo npm docker; do
  command -v "$cmd" >/dev/null || { echo "Missing '$cmd'. Install it first (see README)."; exit 1; }
done

if lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; then
  echo "Port $PORT is already in use. Stop whatever runs there (or set PORT=…)."
  exit 1
fi

echo "› Database (Postgres on port $PG_PORT)"
POSTGRES_PORT="$PG_PORT" docker compose -f "$ROOT/docker-compose.yml" up -d postgres >/dev/null
for _ in $(seq 1 30); do
  docker compose -f "$ROOT/docker-compose.yml" exec -T postgres pg_isready -U partello >/dev/null 2>&1 && break
  sleep 1
done

echo "› Web app build"
( cd "$ROOT/web" && { [ -d node_modules ] || npm ci; } && npm run build >/dev/null )

echo "› Server build (release; the first time takes a few minutes)"
( cd "$ROOT/server" && cargo build --release --quiet )

cleanup() {
  echo
  echo "› Stopping tunnel and server"
  [ -n "${TUNNEL_PID:-}" ] && kill "$TUNNEL_PID" 2>/dev/null || true
  [ -n "${SERVER_PID:-}" ] && kill "$SERVER_PID" 2>/dev/null || true
  [ -n "${AWAKE_PID:-}" ] && kill "$AWAKE_PID" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

echo "› Opening Cloudflare tunnel"
: > "$LOG_DIR/cloudflared.log"
cloudflared tunnel --no-autoupdate --protocol "${TUNNEL_PROTOCOL:-http2}" --url "http://localhost:$PORT" > "$LOG_DIR/cloudflared.log" 2>&1 &
TUNNEL_PID=$!

URL=""
for _ in $(seq 1 60); do
  URL="$(grep -oE 'https://[a-z0-9-]+\.trycloudflare\.com' "$LOG_DIR/cloudflared.log" | head -1 || true)"
  [ -n "$URL" ] && break
  sleep 1
done
if [ -z "$URL" ]; then
  echo "Could not get a tunnel address. Log: $LOG_DIR/cloudflared.log"
  exit 1
fi

echo "› Starting Partello"
(
  cd "$ROOT/server"
  DATABASE_URL="postgres://partello:partello@localhost:$PG_PORT/partello" \
  PORT="$PORT" \
  STATIC_DIR="$ROOT/web/dist" \
  ALLOWED_ORIGINS="$URL,http://localhost:$PORT" \
  PUBLIC_BASE_URL="$URL" \
  TRUST_PROXY=true \
  CLIENT_IP_HEADER=CF-Connecting-IP \
  RUST_LOG="partello=info,tower_http=warn" \
  exec ./target/release/partello-server
) > "$LOG_DIR/server.log" 2>&1 &
SERVER_PID=$!

# Keep the Mac awake while sharing (no-op elsewhere).
if command -v caffeinate >/dev/null; then
  caffeinate -dimsu -w "$SERVER_PID" &
  AWAKE_PID=$!
fi

for _ in $(seq 1 30); do
  curl -fs "http://localhost:$PORT/readyz" >/dev/null 2>&1 && break
  sleep 1
done
curl -fs "http://localhost:$PORT/readyz" >/dev/null || { echo "Server did not start. Log: $LOG_DIR/server.log"; exit 1; }

cat <<EOF

  Partello is online at:

    $URL

  Create a room there (not on localhost) so invite links use this address.
  Logs: $LOG_DIR/  ·  Press Ctrl+C to stop.

EOF

wait "$SERVER_PID"
