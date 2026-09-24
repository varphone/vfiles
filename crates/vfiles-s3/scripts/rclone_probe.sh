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
tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/vfiles-s3-rclone-probe.XXXXXX")
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
export VFILES_WEBDAV_ENABLED=false
export VFILES_S3_ENABLED=true
export VFILES_S3_ACCESS_KEY=rclone-access
export VFILES_S3_SECRET_KEY=rclone-secret-123
export VFILES_S3_REGION=us-east-1
export VFILES_FTP_ENABLED=false
export VFILES_RSYNC_ENABLED=false

"$binary" init >/dev/null
"$binary" user create \
  --username rclones3probe \
  --email rclones3probe@example.invalid \
  --password rclone-s3-probe-password-123 \
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

cat >"$tmpdir/rclone.conf" <<EOF
[s3]
type = s3
provider = Other
env_auth = false
access_key_id = rclone-access
secret_access_key = rclone-secret-123
region = us-east-1
endpoint = http://127.0.0.1:$port
force_path_style = true
EOF
chmod 600 "$tmpdir/rclone.conf"
export RCLONE_CONFIG="$tmpdir/rclone.conf"
export RCLONE_ASK_PASSWORD=false

mkdir -p "$tmpdir/source/nested"
printf 'rclone S3 interoperability payload\n' >"$tmpdir/source/nested/space name-雪.txt"
printf '\000\001\002\377binary\n' >"$tmpdir/source/binary.dat"
dd if=/dev/urandom of="$tmpdir/source/multipart.bin" bs=1M count=11 status=none

rclone lsd s3: | grep -Fq default
rclone copy "$tmpdir/source" s3:default/rclone-probe/tree \
  --create-empty-src-dirs --s3-upload-cutoff 5M --s3-chunk-size 5M --quiet
rclone check "$tmpdir/source" s3:default/rclone-probe/tree --download --quiet
rclone copyto 's3:default/rclone-probe/tree/nested/space name-雪.txt' "$tmpdir/downloaded.txt" --quiet
cmp "$tmpdir/source/nested/space name-雪.txt" "$tmpdir/downloaded.txt"
rclone copyto s3:default/rclone-probe/tree/multipart.bin "$tmpdir/multipart-downloaded.bin" \
  --s3-upload-cutoff 5M --s3-chunk-size 5M --quiet
cmp "$tmpdir/source/multipart.bin" "$tmpdir/multipart-downloaded.bin"

rclone copyto s3:default/rclone-probe/tree/binary.dat "$tmpdir/copied.bin" --quiet
cmp "$tmpdir/source/binary.dat" "$tmpdir/copied.bin"
rclone copyto "$tmpdir/copied.bin" s3:default/rclone-probe/tree/copied.bin --quiet
rclone copyto s3:default/rclone-probe/tree/copied.bin "$tmpdir/uploaded-back.bin" --quiet
cmp "$tmpdir/copied.bin" "$tmpdir/uploaded-back.bin"
rclone deletefile s3:default/rclone-probe/tree/copied.bin --quiet
if rclone lsf s3:default/rclone-probe/tree | grep -Fxq copied.bin; then
  echo "rclone DELETE left the copied S3 object present" >&2
  exit 1
fi

# Exercise a real client continuation token over more than one 1000-key page.
mkdir -p "$tmpdir/pagination"
for index in $(seq -w 0 1004); do
  : >"$tmpdir/pagination/object-$index"
done
rclone sync "$tmpdir/pagination" s3:default/rclone-probe/pagination --quiet
listed=$(rclone lsf s3:default/rclone-probe/pagination --files-only | wc -l)
if [[ "$listed" -ne 1005 ]]; then
  echo "expected 1005 paginated objects, found $listed" >&2
  exit 1
fi
# Temporary server storage is removed by the EXIT trap.
echo "PASS rclone $(rclone version | awk 'NR == 1 { print $2 }') S3 multipart/check/copy/move/delete/1005-key pagination"
