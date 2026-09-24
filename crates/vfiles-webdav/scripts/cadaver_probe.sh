#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
cd "$repo_root"

for command in cargo cadaver curl python3 script; do
  command -v "$command" >/dev/null || {
    echo "required command not found: $command" >&2
    exit 2
  }
done

cargo build -q -p vfiles-bin --bin vfiles
binary="$repo_root/target/debug/vfiles"
tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/vfiles-cadaver-probe.XXXXXX")
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
export VFILES_HTTP_PORT="$port"
export VFILES_HTTP_CORS_ALLOWED_ORIGINS=http://webdav-client.example.invalid
export VFILES_AUTH_ENABLED=true
export VFILES_WEBDAV_ENABLED=true
export VFILES_S3_ENABLED=false
export VFILES_FTP_ENABLED=false
export VFILES_RSYNC_ENABLED=false

"$binary" init >/dev/null
"$binary" user create \
  --username cadprobe \
  --email cadprobe@example.invalid \
  --password cadprobe-password-123 \
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

curl --fail --silent --show-error -D "$tmpdir/preflight.headers" -o /dev/null \
  -X OPTIONS "http://127.0.0.1:$port/dav" \
  -H 'Origin: http://webdav-client.example.invalid' \
  -H 'Access-Control-Request-Method: PROPFIND' \
  -H 'Access-Control-Request-Headers: authorization,depth,destination,overwrite,if,lock-token,timeout'
python3 - "$tmpdir/preflight.headers" <<'PY'
import sys

headers = {}
for line in open(sys.argv[1], encoding="ascii"):
    if ":" in line:
        name, value = line.split(":", 1)
        headers[name.strip().lower()] = value.strip().lower()

assert headers.get("access-control-allow-origin") == "http://webdav-client.example.invalid", headers
methods = {item.strip() for item in headers.get("access-control-allow-methods", "").split(",")}
assert "propfind" in methods, headers
allowed = {item.strip() for item in headers.get("access-control-allow-headers", "").split(",")}
required = {"authorization", "depth", "destination", "overwrite", "if", "lock-token", "timeout"}
assert required <= allowed, (required - allowed, headers)
PY

mkdir -p "$tmpdir/home"
printf 'machine 127.0.0.1 login cadprobe password cadprobe-password-123\n' \
  >"$tmpdir/home/.netrc"
chmod 700 "$tmpdir/home"
chmod 600 "$tmpdir/home/.netrc"
printf 'cadaver real-client payload\n' >"$tmpdir/source.txt"
cat >"$tmpdir/commands" <<EOF
mkcol cadprobe
cd cadprobe
put $tmpdir/source.txt uploaded.txt
copy uploaded.txt copied.txt
move copied.txt moved.txt
get moved.txt $tmpdir/downloaded.txt
ls
delete uploaded.txt
delete moved.txt
cd ..
rmcol cadprobe
quit
EOF

HOME="$tmpdir/home" script -qefc \
  "cadaver http://127.0.0.1:$port/dav" /dev/null \
  <"$tmpdir/commands" >"$tmpdir/cadaver.log" 2>&1

cmp "$tmpdir/source.txt" "$tmpdir/downloaded.txt"
grep -Fq "Creating" "$tmpdir/cadaver.log"
grep -Fq "Copying" "$tmpdir/cadaver.log"
grep -Fq "Moving" "$tmpdir/cadaver.log"
grep -Fq "Listing collection" "$tmpdir/cadaver.log"
grep -Fq "Deleting collection" "$tmpdir/cadaver.log"
if grep -Eiq "failed|error|aborted" "$tmpdir/cadaver.log"; then
  cat "$tmpdir/cadaver.log" >&2
  exit 1
fi

echo "PASS WebDAV CORS PROPFIND preflight and cadaver $(cadaver --version 2>&1 | awk 'NR == 1 { print $2 }') MKCOL/PUT/COPY/MOVE/GET/list/DELETE/rmcol"
