#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLIENT_DIR="${ROOT_DIR}/apps/desktop-client"

cleanup() {
  echo ""
  echo "Stopping dev processes..."
  for pid in ${PIDS:-}; do
    if kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
    fi
  done
}

trap cleanup EXIT INT TERM

echo "Starting signaling server..."
(
  cd "$ROOT_DIR"
  cargo run -p signaling-server
) &
PIDS="$!"

echo "Starting client A on port 5173..."
(
  cd "$CLIENT_DIR"
  VITE_PORT=5173 npm run tauri dev
) &
PIDS="$PIDS $!"

echo "Starting client B on port 5174..."
(
  cd "$CLIENT_DIR"
  VITE_PORT=5174 npm run tauri dev
) &
PIDS="$PIDS $!"

wait
