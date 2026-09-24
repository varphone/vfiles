#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
cd "$repo_root"

for command in cargo curl litmus python3; do
  command -v "$command" >/dev/null || {
    echo "required command not found: $command" >&2
    exit 2
  }
done

cargo build -q -p vfiles-bin --bin vfiles
binary="$repo_root/target/debug/vfiles"
tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/vfiles-litmus-probe.XXXXXX")
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
  --username litmusprobe \
  --email litmusprobe@example.invalid \
  --password litmusprobe-password-123 \
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

options_headers=$(curl --fail --silent --show-error --dump-header - --output /dev/null \
  --request OPTIONS "http://127.0.0.1:$port/dav/" | tr '[:upper:]' '[:lower:]')
grep -Eq '^allow:.*(^|[,[:space:]])lock([,[:space:]]|$)' <<<"$options_headers" || {
  echo "WebDAV OPTIONS did not advertise LOCK in Allow" >&2
  exit 1
}
grep -Eq '^dav:.*(^|[,[:space:]])2([,[:space:]]|$)' <<<"$options_headers" || {
  echo "WebDAV OPTIONS did not advertise DAV class 2" >&2
  exit 1
}

dav_url="http://127.0.0.1:$port/dav"
curl --fail --silent --show-error --user litmusprobe:litmusprobe-password-123 \
  --request MKCOL "$dav_url/litmus-infinity/" >/dev/null
curl --fail --silent --show-error --user litmusprobe:litmusprobe-password-123 \
  --request MKCOL "$dav_url/litmus-infinity/sub/" >/dev/null
printf 'infinity depth probe\n' >"$tmpdir/infinity.txt"
curl --fail --silent --show-error --user litmusprobe:litmusprobe-password-123 \
  --upload-file "$tmpdir/infinity.txt" "$dav_url/litmus-infinity/sub/infinity.txt" >/dev/null
infinity_status=$(curl --silent --show-error --user litmusprobe:litmusprobe-password-123 \
  --request PROPFIND --header 'Depth: infinity' \
  --output "$tmpdir/infinity.xml" --write-out '%{http_code}' "$dav_url/litmus-infinity/")
default_depth_status=$(curl --silent --show-error --user litmusprobe:litmusprobe-password-123 \
  --request PROPFIND --output "$tmpdir/default-depth.xml" --write-out '%{http_code}' "$dav_url/")
python3 - "$tmpdir/infinity.xml" "$tmpdir/default-depth.xml" \
  "$infinity_status" "$default_depth_status" <<'PY'
import sys
import xml.etree.ElementTree as ET

infinity_path, default_path, infinity_status, default_status = sys.argv[1:]
if infinity_status != "207" or default_status != "207":
    raise SystemExit(
        f"expected Depth infinity/default statuses 207; got {infinity_status}/{default_status}"
    )
ns = {"d": "DAV:"}
for path, expected in (
    (infinity_path, "/dav/litmus-infinity/sub/infinity.txt"),
    (default_path, "/dav/litmus-infinity/sub/infinity.txt"),
):
    root = ET.parse(path).getroot()
    hrefs = {element.text for element in root.findall("d:response/d:href", ns)}
    if expected not in hrefs:
        raise SystemExit(f"{path}: recursive resource {expected!r} missing from {hrefs!r}")
PY

if ! litmus "http://127.0.0.1:$port/dav" litmusprobe litmusprobe-password-123 \
  >"$tmpdir/litmus.log" 2>&1; then
  cat "$tmpdir/litmus.log" >&2
  cat "$tmpdir/server.log" >&2
  exit 1
fi
cat "$tmpdir/litmus.log"
python3 - "$tmpdir/litmus.log" <<'PY'
import re
import sys

output = open(sys.argv[1], encoding="utf-8").read()
if "DELETE removed collection resource with Request-URI including fragment" in output:
    raise SystemExit("litmus detected a fragment-bearing DELETE reached the resource")
summaries = re.findall(
    r"of (\d+) tests run: (\d+) passed, (\d+) failed", output
)
run = sum(int(item[0]) for item in summaries)
passed = sum(int(item[1]) for item in summaries)
failed = sum(int(item[2]) for item in summaries)
if (run, passed, failed) != (104, 104, 0):
    raise SystemExit(
        f"expected 104/104 litmus checks to pass; got {passed}/{run}, "
        f"{failed} failed across {len(summaries)} suites"
    )
PY

echo "PASS litmus all 104 checks"
