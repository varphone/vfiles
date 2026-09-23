#!/usr/bin/env python3
"""S3 SigV4 实证探针（r2 九式 + r3 列表/Range 八式 = 入仓可复演 ✓ AWS SigV4 标准算法）。

用法：起服（VFILES_S3_ENABLED=true + 单对密钥）后
  python3 crates/vfiles-s3/scripts/sigv4_probe.py http://127.0.0.1:9000 <access> <secret> <db路径>
可选第二入口（真 AWS SDK，需 `pip install boto3`）：scripts/boto_probe.py
"""
import base64
import hashlib
import hmac
import re
import sqlite3
import sys
import urllib.error
import urllib.parse
import urllib.request
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

def q(params):
    """规范查询串：按 key 排序 + 值 URL 编码。"""
    return "&".join(f"{k}={urllib.parse.quote(str(v), safe='')}" for k, v in sorted(params.items()))

def keys_of(body):
    return re.findall(rb"<Contents>.*?<Key>(.*?)</Key>", body, re.S)

def prefixes_of(body):
    return re.findall(rb"<CommonPrefixes>.*?<Prefix>(.*?)</Prefix>", body, re.S)

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
    # ── r2 九式 ──
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

    # ── r3 列表 / 元数据 / Range 八式 ──
    base = "/default"
    blob = b"0123456789abcdef"
    seed = [("probe2/a.txt", b"aa"), ("probe2/b.txt", b"bb"),
            ("probe2/dir/x.txt", b"xx"), ("probe2/dir/y.txt", b"yy"),
            ("probe2/range.bin", blob)]
    for k, b in seed:
        request("PUT", base + "/" + k, body=b)

    st, body, _ = request("GET", base, query=q({"list-type": "2", "prefix": "probe2/"}))
    has_size = b"<Size>2</Size>" in body
    has_lm = re.search(rb"<LastModified>\d{4}-\d\d-\d\dT", body) is not None
    has_etag = re.search(rb"<ETag>(&quot;|\")[0-9a-f]{32}(&quot;|\")</ETag>", body) is not None
    check("10 Contents metadata size/lastmodified/etag",
          st == 200 and has_size and has_lm and has_etag,
          f"{st} size={has_size} lm={has_lm} etag={has_etag}")

    st, body, _ = request("GET", base, query=q({"list-type": "2", "prefix": "probe2/", "delimiter": "/"}))
    ks, ps = keys_of(body), prefixes_of(body)
    check("11 ListObjectsV2 delimiter CommonPrefixes",
          st == 200 and ps == [b"probe2/dir/"]
          and sorted(ks) == [b"probe2/a.txt", b"probe2/b.txt", b"probe2/range.bin"],
          f"{st} keys={ks} prefixes={ps}")

    st, body, _ = request("GET", base, query=q({"list-type": "2", "prefix": "probe2/", "max-keys": "1"}))
    trunc = b"<IsTruncated>true</IsTruncated>" in body
    m = re.search(rb"<NextContinuationToken>(.*?)</NextContinuationToken>", body)
    first = keys_of(body)
    check("12a page1 truncated + token", st == 200 and trunc and m is not None and len(first) == 1, f"{st} {first}")
    seen = list(first)
    tok = m.group(1).decode() if m else ""
    for _ in range(8):
        st, body, _ = request("GET", base, query=q({
            "list-type": "2", "prefix": "probe2/", "max-keys": "1", "continuation-token": tok}))
        seen += keys_of(body)
        m = re.search(rb"<NextContinuationToken>(.*?)</NextContinuationToken>", body)
        if m is None or b"<IsTruncated>true</IsTruncated>" not in body:
            break
        tok = m.group(1).decode()
    check("12b pagination complete no dup", len(seen) == len(set(seen)) and len(seen) >= 5, f"seen={seen}")

    st, body, _ = request("GET", base, query=q({"prefix": "probe2/", "max-keys": "3"}))
    check("13 ListObjects V1 Contents+IsTruncated",
          st == 200 and b"ListBucketResult" in body and len(keys_of(body)) == 3
          and b"<IsTruncated>true</IsTruncated>" in body,
          f"{st} keys={len(keys_of(body))}")

    st, body, h = request("GET", base + "/probe2/range.bin", extra_headers={"range": "bytes=2-5"})
    cr = h.get("Content-Range") or h.get("content-range")
    check("14 Range 206 + Content-Range",
          st == 206 and body == blob[2:6] and cr == "bytes 2-5/16", f"{st} body={body} cr={cr}")

    st, body, _ = request("GET", base + "/probe2/range.bin", extra_headers={"range": "bytes=999999-"})
    check("15 Range 416 InvalidRange", st == 416 and b"InvalidRange" in body, st)

    st, _, h = request("HEAD", base + "/probe2/a.txt")
    lm = h.get("Last-Modified") or h.get("last-modified")
    check("16 HEAD Last-Modified", st == 200 and lm is not None, f"{st} lm={lm}")

    # ── r4 multipart 四式 ──
    mp_key = "probe3/mp.bin"
    st, body, _ = request("POST", base + "/" + mp_key, query=q({"uploads": ""}))
    m = re.search(rb"<UploadId>(.*?)</UploadId>", body)
    uid = m.group(1).decode() if m else ""
    check("17 CreateMultipartUpload returns UploadId", st == 200 and bool(uid), f"{st} uid={uid[:8]}")

    p1, p2 = b"A" * 700, b"B" * 500
    st1, _, h1 = request("PUT", base + "/" + mp_key, query=q({"partNumber": "1", "uploadId": uid}), body=p1)
    st2, _, h2 = request("PUT", base + "/" + mp_key, query=q({"partNumber": "2", "uploadId": uid}), body=p2)
    e1 = h1.get("ETag") or h1.get("etag")
    e2 = h2.get("ETag") or h2.get("etag")
    check("18 UploadPart returns ETag", st1 == 200 and st2 == 200 and bool(e1) and bool(e2), f"{st1}/{st2}")

    xml = (
        "<CompleteMultipartUpload>"
        f"<Part><PartNumber>1</PartNumber><ETag>{e1}</ETag></Part>"
        f"<Part><PartNumber>2</PartNumber><ETag>{e2}</ETag></Part>"
        "</CompleteMultipartUpload>"
    ).encode()
    st, body, _ = request("POST", base + "/" + mp_key, query=q({"uploadId": uid}), body=xml,
                          extra_headers={"content-type": "application/xml"})
    ok_complete = st == 200 and b"<ETag>" in body
    stg, gbody, _ = request("GET", base + "/" + mp_key)
    check("19 CompleteMultipartUpload assembles bytes",
          ok_complete and stg == 200 and gbody == p1 + p2, f"{st} get={stg} bytes={len(gbody)}")

    mp_abort = "probe3/abort.bin"
    st, abody, _ = request("POST", base + "/" + mp_abort, query=q({"uploads": ""}))
    m2 = re.search(rb"<UploadId>(.*?)</UploadId>", abody)
    uid2 = m2.group(1).decode() if m2 else ""
    request("PUT", base + "/" + mp_abort, query=q({"partNumber": "1", "uploadId": uid2}), body=b"x" * 10)
    st, _, _ = request("DELETE", base + "/" + mp_abort, query=q({"uploadId": uid2}))
    check("20 AbortMultipartUpload 204", st == 204, st)

    # ── r6 批量删（DeleteObjects ✗ Content-MD5 必需）──
    del_keys = ["probe4/d1.txt", "probe4/d2.txt"]
    for k in del_keys:
        request("PUT", base + "/" + k, body=b"z")
    dxml = (
        "<Delete>"
        + "".join(f"<Object><Key>{k}</Key></Object>" for k in del_keys + ["probe4/missing.txt"])
        + "</Delete>"
    ).encode()
    dmd5 = base64.b64encode(hashlib.md5(dxml).digest()).decode()
    st, dbody, _ = request(
        "POST",
        base,
        query=q({"delete": ""}),
        body=dxml,
        extra_headers={"content-type": "application/xml", "content-md5": dmd5},
    )
    ndel = dbody.count(b"<Deleted>")
    check(
        "21 DeleteObjects 3 deleted (incl. missing)",
        st == 200 and ndel == 3 and b"<Error>" not in dbody,
        f"{st} n={ndel}",
    )
    st, lbody, _ = request("GET", base, query=q({"list-type": "2", "prefix": "probe4/"}))
    check("22 DeleteObjects removed keys", st == 200 and len(keys_of(lbody)) == 0, keys_of(lbody))

    # ── r10 服务端复制（CopyObject ✗ PUT + x-amz-copy-source）──
    cblob = b"copy-src-bytes"
    request("PUT", base + "/probe5/src.bin", body=cblob,
            extra_headers={"content-type": "application/x-thing"})
    st, _, _ = request("PUT", base + "/probe5/dst.bin",
                       extra_headers={"x-amz-copy-source": "/default/probe5/src.bin"})
    st2, gbody, gh = request("GET", base + "/probe5/dst.bin")
    ct = gh.get("Content-Type") or gh.get("content-type")
    check("23 CopyObject bytes + ContentType",
          st == 200 and st2 == 200 and gbody == cblob and ct == "application/x-thing",
          f"{st}/{st2} ct={ct}")
    request("DELETE", base + "/probe5/src.bin")
    request("DELETE", base + "/probe5/dst.bin")

    # 清理
    for k, _ in seed:
        request("DELETE", base + "/" + k)
    request("DELETE", base + "/" + mp_key)

    passed = sum(1 for x in P if x)
    print(f"== {passed}/{len(P)} PASS ==")
    sys.exit(0 if passed == len(P) else 1)

if __name__ == "__main__":
    main()
