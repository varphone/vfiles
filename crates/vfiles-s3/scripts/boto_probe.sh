#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
cd "$repo_root"

for command in cargo curl python3; do
  command -v "$command" >/dev/null || {
    echo "required command not found: $command" >&2
    exit 2
  }
done
python3 -c 'import boto3' || {
  echo "python3 boto3 is required (install python3-boto3)" >&2
  exit 2
}

cargo build -q -p vfiles-bin --bin vfiles
binary="$repo_root/target/debug/vfiles"
tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/vfiles-boto-probe.XXXXXX")
server_pid=
cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf -- "$tmpdir"
}
trap cleanup EXIT

port=$(python3 - <<'PY'
import socket

with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)

export VFILES_STORAGE_ROOT="$tmpdir/storage"
export VFILES_DATABASE_PATH="$tmpdir/storage/vfiles.db"
export VFILES_HTTP_HOST=127.0.0.1
export VFILES_AUTH_ENABLED=true
export VFILES_WEBDAV_ENABLED=false
export VFILES_S3_ENABLED=true
export VFILES_S3_ACCESS_KEY=test-access
export VFILES_S3_SECRET_KEY=test-secret-123
export VFILES_S3_REGION=us-east-1
export VFILES_FTP_ENABLED=false
export VFILES_RSYNC_ENABLED=false

"$binary" init >/dev/null
"$binary" user create \
  --username botoprobe \
  --email botoprobe@example.invalid \
  --password boto-probe-password-123 \
  --role admin >/dev/null

"$binary" serve --host 127.0.0.1 --port "$port" >"$tmpdir/server.log" 2>&1 &
server_pid=$!
ready=false
for _ in $(seq 1 100); do
  if curl --fail --silent "http://127.0.0.1:$port/api/health" >/dev/null; then
    ready=true
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    cat "$tmpdir/server.log" >&2
    exit 1
  fi
  sleep 0.1
done
if [[ "$ready" != true ]]; then
  cat "$tmpdir/server.log" >&2
  echo "VFiles did not become ready" >&2
  exit 1
fi

if ! python3 crates/vfiles-s3/scripts/boto_probe.py \
  "http://127.0.0.1:$port" test-access test-secret-123 \
  >"$tmpdir/boto.log" 2>&1; then
  cat "$tmpdir/boto.log" >&2
  cat "$tmpdir/server.log" >&2
  exit 1
fi
cat "$tmpdir/boto.log"
