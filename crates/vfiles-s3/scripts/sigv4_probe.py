#!/usr/bin/env python3
"""S3 SigV4 九式实证探针（round 2/256 入仓可复演 ✓ AWS SigV4 标准算法）。"""
import hashlib
import hmac
import sqlite3
import sys
import urllib.request
import urllib.error
from datetime import datetime, timezone

ENDPOINT = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:9000"
ACCESS = sys.argv[2] if len(sys.argv) > 2 else "test-access"
SECRET = sys.argv[3] if len(sys.argv) > 3 else "test-secret-123"
DB = sys.argv[4] if len(sys.argv) > 4 else ""
REGION = "us-east-1"
SERVICE = "s3"

def _sign(key, msg):
    return hmac.new(key, msg.encode("utf-8"), hashlib.sha256).digest()

def sigv4_sign(method, path, query, headers, payload_sha, secret):
    host = ENDPOINT.split("//", 1)[1]
    now = datetime.now(timezone.utc)
    amz_date = now.strftime("%Y%m%dT%H%M%SZ")
    date_stamp = now.strftime("%Y%m%d")
    headers = dict(headers)
    headers["host"] = host
    headers["x-amz-date"] = amz_date
    headers["x-amz-content-sha256"] = payload_sha
    signed_names = sorted(k.lower() for k in headers)
    canon = "".join(f"{k.lower()}:{' '.join(str(headers[k]).split())}\n" for k in signed_names)
    canonical_request = "\n".join([
        method, path, query or "", canon, ";".join(signed_names), payload_sha,
    ])
    scope = f"{date_stamp}/{REGION}/{SERVICE}/aws4_request"
    sts = "\n".join([
        "AWS4-HMAC-SHA256", amz_date, scope,
        hashlib.sha256(canonical_request.encode()).hexdigest(),
    ])
    k = _sign(("AWS4" + secret).encode(), date_stamp)
    k = _sign(k, REGION)
    k = _sign(k, SERVICE)
    k = _sign(k, "aws4_request")
    sig = hmac.new(k, sts.encode(), hashlib.sha256).hexdigest()
    headers["authorization"] = (
        f"AWS4-HMAC-SHA256 Credential={ACCESS}/{scope}, "
        f"SignedHeaders={';'.join(signed_names)}, Signature={sig}"
    )
    return headers

def request(method, path, query="", body=b"", extra_headers=None, secret=None):
    url = ENDPOINT + path + (("?" + query) if query else "")
    payload = body
    payload_sha = hashlib.sha256(payload).hexdigest()
    h = dict(extra_headers or {})
    secret_eff = SECRET if secret is None else secret
    headers = sigv4_sign(method, path, query, h, payload_sha, secret_eff)
    req = urllib.request.Request(url, data=payload if payload else None, method=method, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            return resp.status, resp.read(), dict(resp.headers)
    except urllib.error.HTTPError as e:
        return e.code, e.read(), dict(e.headers)

P = []

def check(name, ok, detail=""):
    P.append(ok)
    print(("PASS " if ok else "FAIL ") + name + (" | " + str(detail) if detail else ""))

def ver_count(key):
    if not DB:
        return -1
    con = sqlite3.connect(DB)
    try:
        n = con.execute(
            "SELECT count(*) FROM entry_versions ev JOIN entries e ON e.id=ev.entry_id WHERE e.path=?",
            (key,),
        ).fetchone()[0]
    finally:
        con.close()
    return n

def main():
    st, body, _ = request("GET", "/")
    check("1 ListBuckets default", st == 200 and b"<Name>default</Name>" in body, st)

    st, body, _ = request("GET", "/default", query="list-type=2&prefix=probe%2F")
    check("2 ListObjectsV2", st == 200 and b"ListBucketResult" in body, st)

    key = "probe/r1.txt"
    v0 = ver_count(key)
    st, _, _ = request("PUT", "/default/" + key, body=b"content-v1", extra_headers={"content-type": "text/plain"})
    v1 = ver_count(key)
    check("3 PUT 200 +version", st == 200 and (v0 < 0 or v1 == v0 + 1), f"{st} v{v0}->{v1}")

    st, _, _ = request("PUT", "/default/" + key, body=b"content-v2-longer")
    v2 = ver_count(key)
    check("4 PUT same key +version", st == 200 and (v1 < 0 or v2 == v1 + 1), f"{st} v{v1}->{v2}")

    st, body, hdrs = request("GET", "/default/" + key)
    etag = hdrs.get("ETag") or hdrs.get("etag")
    check("5 GET content+ETag", st == 200 and body in (b"content-v1", b"content-v2-longer") and bool(etag), f"{st} etag={etag}")

    st, _, hdrs = request("HEAD", "/default/" + key)
    cl = hdrs.get("Content-Length") or hdrs.get("content-length")
    check("6 HEAD CL", st == 200 and cl is not None, f"{st} cl={cl}")

    st, _, _ = request("DELETE", "/default/" + key)
    st2, _, _ = request("DELETE", "/default/" + key)
    check("7 DELETE idempotent 204", st == 204 and st2 == 204, f"{st}/{st2}")

    st, body, _ = request("GET", "/wrongbucket/x.txt")
    check("8 NoSuchBucket", st == 404 and b"NoSuchBucket" in body, st)

    st, _, _ = request("GET", "/", secret="totally-wrong")
    check("9 wrong secret 403", st == 403, st)

    passed = sum(1 for x in P if x)
    print(f"== {passed}/{len(P)} PASS ==")
    sys.exit(0 if passed == len(P) else 1)

if __name__ == "__main__":
    main()
