#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
cd "$repo_root"

for command in cargo curl python3 rclone; do
  command -v "$command" >/dev/null || {
    echo "required command not found: $command" >&2
    exit 2
  }
done

cargo build -q -p vfiles-bin --bin vfiles
binary="$repo_root/target/debug/vfiles"
tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/vfiles-rclone-probe.XXXXXX")
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
export VFILES_AUTH_ENABLED=true
export VFILES_WEBDAV_ENABLED=true
export VFILES_S3_ENABLED=false
export VFILES_FTP_ENABLED=false
export VFILES_RSYNC_ENABLED=false

"$binary" init >/dev/null
"$binary" user create \
  --username rcloneprobe \
  --email rcloneprobe@example.invalid \
  --password rclone-probe-password-123 \
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

password=$(printf '%s\n' 'rclone-probe-password-123' | rclone obscure -)
cat >"$tmpdir/rclone.conf" <<EOF
[dav]
type = webdav
url = http://127.0.0.1:$port/dav
vendor = other
user = rcloneprobe
pass = $password
EOF
export RCLONE_CONFIG="$tmpdir/rclone.conf"
export RCLONE_ASK_PASSWORD=false

mkdir -p "$tmpdir/source/nested" "$tmpdir/source/empty-source-dir"
printf 'rclone WebDAV interoperability payload\n' >"$tmpdir/source/nested/space name-雪.txt"
printf '\000\001\002\377binary\n' >"$tmpdir/source/binary.dat"
rclone mkdir dav:probe/empty-directory
rclone copy "$tmpdir/source" dav:probe/tree --create-empty-src-dirs --quiet
rclone check "$tmpdir/source" dav:probe/tree --download --quiet
rclone lsf dav:probe/tree --dirs-only --recursive | grep -Fxq empty-source-dir/

rclone copyto 'dav:probe/tree/nested/space name-雪.txt' "$tmpdir/downloaded.txt" --quiet
cmp "$tmpdir/source/nested/space name-雪.txt" "$tmpdir/downloaded.txt"
rclone moveto 'dav:probe/tree/nested/space name-雪.txt' dav:probe/tree/nested/moved.txt --quiet
rclone copyto dav:probe/tree/nested/moved.txt "$tmpdir/moved.txt" --quiet
cmp "$tmpdir/source/nested/space name-雪.txt" "$tmpdir/moved.txt"
rclone deletefile dav:probe/tree/nested/moved.txt --quiet
if rclone lsf dav:probe/tree/nested --files-only | grep -Fxq moved.txt; then
  echo "rclone DELETE left the moved file present" >&2
  exit 1
fi
rclone purge dav:probe/tree
rclone rmdir dav:probe/empty-directory

echo "PASS rclone $(rclone version | awk 'NR == 1 { print $2 }') WebDAV MKCOL/PUT/GET/check/MOVE/DELETE"
