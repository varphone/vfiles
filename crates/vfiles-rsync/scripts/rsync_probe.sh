#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
cd "$repo_root"

for command in cargo curl python3 rsync; do
  command -v "$command" >/dev/null || {
    echo "required command not found: $command" >&2
    exit 2
  }
done

cargo build -q -p vfiles-bin --bin vfiles
binary="$repo_root/target/debug/vfiles"
tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/vfiles-rsync-probe.XXXXXX")
server_pid=
cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf -- "$tmpdir"
}
trap cleanup EXIT

readarray -t ports < <(python3 - <<'PY'
import socket

for _ in range(2):
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        print(sock.getsockname()[1])
PY
)
http_port=${ports[0]}
rsync_port=${ports[1]}

export VFILES_STORAGE_ROOT="$tmpdir/storage"
export VFILES_DATABASE_PATH="$tmpdir/storage/vfiles.db"
export VFILES_HTTP_HOST=127.0.0.1
export VFILES_HTTP_PORT="$http_port"
export VFILES_AUTH_ENABLED=false
export VFILES_WEBDAV_ENABLED=false
export VFILES_S3_ENABLED=false
export VFILES_FTP_ENABLED=false
export VFILES_RSYNC_ENABLED=true
export VFILES_RSYNC_PORT="$rsync_port"
export VFILES_RSYNC_MODULE=files
export VFILES_RSYNC_WRITABLE=true

"$binary" init >/dev/null
"$binary" user create \
  --username rsyncprobe \
  --email rsyncprobe@example.invalid \
  --password rsyncprobe-password-123 \
  --role admin >/dev/null

"$binary" serve --host 127.0.0.1 --port "$http_port" >"$tmpdir/server.log" 2>&1 &
server_pid=$!
rsync_url="rsync://127.0.0.1:$rsync_port/"
ready=false
for _ in $(seq 1 100); do
  if curl --fail --silent "http://127.0.0.1:$http_port/api/health" >/dev/null; then
    if rsync --list-only "$rsync_url" >"$tmpdir/modules.log" 2>/dev/null; then
      ready=true
      break
    fi
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    cat "$tmpdir/server.log" >&2
    exit 1
  fi
  sleep 0.1
done
if [[ "$ready" != true ]]; then
  cat "$tmpdir/server.log" >&2
  echo "VFiles rsync daemon did not become ready" >&2
  exit 1
fi

grep -Eq '^files[[:space:]]' "$tmpdir/modules.log" || {
  cat "$tmpdir/modules.log" >&2
  echo "expected the files module in daemon discovery" >&2
  exit 1
}
module_url="rsync://127.0.0.1:$rsync_port/files/"
mkdir -p "$tmpdir/source/empty" "$tmpdir/source/nested"
python3 - "$tmpdir/source/nested/large.bin" <<'PY'
import pathlib
import sys

pathlib.Path(sys.argv[1]).write_bytes(bytes(range(256)) * 8192)
PY

rsync -a --quiet "$tmpdir/source/" "$module_url"
rsync --list-only "$module_url" >"$tmpdir/initial-list.log"
grep -Fq 'empty' "$tmpdir/initial-list.log"
grep -Fq 'nested' "$tmpdir/initial-list.log"
mkdir -p "$tmpdir/pull"
rsync -a --quiet "$module_url" "$tmpdir/pull/"
diff -r "$tmpdir/source" "$tmpdir/pull"

# Mutate a small range without changing the file size. Checksum mode must find it
# and the server's delta path must reconstruct bytes identical to the source.
python3 - "$tmpdir/source/nested/large.bin" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
with path.open("r+b") as file:
    file.seek(1024 * 1024)
    file.write(b"checksum-delta-probe")
PY
rsync -a --checksum --stats "$tmpdir/source/" "$module_url" >"$tmpdir/delta.log"
python3 - "$tmpdir/delta.log" <<'PY'
import re
import sys

output = open(sys.argv[1], encoding="utf-8").read()
match = re.search(r"Literal data:\s+([\d,]+) bytes", output)
if not match:
    raise SystemExit("rsync --stats did not report literal bytes")
literal_bytes = int(match.group(1).replace(",", ""))
if not 0 < literal_bytes < 2 * 1024 * 1024:
    raise SystemExit(
        f"expected checksum delta to send less than the 2 MiB file; sent {literal_bytes} literal bytes"
    )
PY
mkdir -p "$tmpdir/pull-after-delta"
rsync -a --quiet "$module_url" "$tmpdir/pull-after-delta/"
diff -r "$tmpdir/source" "$tmpdir/pull-after-delta"

# Unsupported entry types must fail the whole push before regular sibling files
# are written; silently skipping a symlink would make rsync report a false success.
printf 'must-not-be-partially-written\n' >"$tmpdir/source/would-be-partial.txt"
ln -s nested/large.bin "$tmpdir/source/unsupported-link"
if rsync -a --quiet "$tmpdir/source/" "$module_url" >"$tmpdir/unsupported.log" 2>&1; then
  echo "rsync push unexpectedly accepted a symlink" >&2
  exit 1
fi
rsync --list-only "$module_url" >"$tmpdir/unsupported-list.log"
if grep -Fq 'would-be-partial.txt' "$tmpdir/unsupported-list.log"; then
  echo "unsupported-entry push partially wrote its regular sibling" >&2
  exit 1
fi
rm "$tmpdir/source/unsupported-link" "$tmpdir/source/would-be-partial.txt"

# Receiver-side exclude rules protect matching destination-only files under
# --delete; --delete-excluded removes them when explicitly requested.
mkdir -p "$tmpdir/destination-only"
printf 'keep\n' >"$tmpdir/destination-only/keep.probe"
printf 'remove\n' >"$tmpdir/destination-only/remove.txt"
rsync -a --quiet "$tmpdir/destination-only/" "$module_url"
rsync -a --exclude='keep.probe' --delete --quiet "$tmpdir/source/" "$module_url"
rsync --list-only "$module_url" >"$tmpdir/protected-list.log"
grep -Fq 'keep.probe' "$tmpdir/protected-list.log"
if grep -Fq 'remove.txt' "$tmpdir/protected-list.log"; then
  echo "--delete failed to remove an unprotected destination-only file" >&2
  exit 1
fi
rsync -a --exclude='keep.probe' --delete-excluded --quiet "$tmpdir/source/" "$module_url"
rsync --list-only "$module_url" >"$tmpdir/deleted-list.log"
if grep -Fq 'keep.probe' "$tmpdir/deleted-list.log"; then
  echo "--delete-excluded failed to remove the excluded file" >&2
  exit 1
fi

echo "PASS rsync $(rsync --version | awk 'NR == 1 { print $3 }') module/push/pull/delta/delete"
