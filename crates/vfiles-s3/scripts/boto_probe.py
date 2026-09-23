#!/usr/bin/env python3
"""S3 真客户端实证（可选第二入口 ✗ 需 `pip install boto3` = 官方 AWS SDK 独立校验）。

与 `sigv4_probe.py` 的自写 SigV4 形成双通道：本脚本由 botocore 负责签名 / XML 解析 /
分页器 / Range，验证我们的响应对真实 SDK 的兼容性（含 SDK 驱动的 paginator 无重无漏）。

用法：起服后 `python3 crates/vfiles-s3/scripts/boto_probe.py http://127.0.0.1:9000`
"""
import sys

import boto3
from botocore.config import Config
from botocore.exceptions import ClientError

ENDPOINT = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:9000"
ACCESS = sys.argv[2] if len(sys.argv) > 2 else "test-access"
SECRET = sys.argv[3] if len(sys.argv) > 3 else "test-secret-123"
ACCESS2 = sys.argv[4] if len(sys.argv) > 4 else ""
SECRET2 = sys.argv[5] if len(sys.argv) > 5 else ""
SECOND_RO = len(sys.argv) > 6 and sys.argv[6] not in ("", "0")
# 可选：第 7/8 参 = 绑定到另一命名空间的凭证对（验多租户隔离 ✗ r26）
NS_ACCESS = sys.argv[7] if len(sys.argv) > 7 else ""
NS_SECRET = sys.argv[8] if len(sys.argv) > 8 else ""

P = []

def check(name, ok, detail=""):
    P.append(ok)
    print(("PASS " if ok else "FAIL ") + name + (" | " + str(detail) if detail else ""))

def main():
    s3 = boto3.client(
        "s3",
        endpoint_url=ENDPOINT,
        aws_access_key_id=ACCESS,
        aws_secret_access_key=SECRET,
        region_name="us-east-1",
        config=Config(s3={"addressing_style": "path"}, retries={"max_attempts": 1}),
    )

    buckets = [b["Name"] for b in s3.list_buckets()["Buckets"]]
    check("boto ListBuckets", buckets == ["default"], buckets)
    # 桶级探测（r25）：rclone / aws-cli 连接检查路径
    try:
        hb = s3.head_bucket(Bucket="default")["ResponseMetadata"]["HTTPStatusCode"] == 200
    except ClientError:
        hb = False
    try:
        s3.head_bucket(Bucket="missing-bucket")
        hb404 = False
    except ClientError as e:
        hb404 = e.response["ResponseMetadata"]["HTTPStatusCode"] == 404
    loc = s3.get_bucket_location(Bucket="default").get("LocationConstraint")
    ver = s3.get_bucket_versioning(Bucket="default")
    check("boto HeadBucket / location / versioning",
          hb and hb404 and loc == "us-east-1" and ver.get("Status") is None,
          f"head={hb} 404={hb404} loc={loc} ver={ver.get('Status')}")

    blob = b"0123456789abcdef"
    seed = [("boto/a.txt", b"aa"), ("boto/b.txt", b"bb"),
            ("boto/dir/x.txt", b"xx"), ("boto/range.bin", blob)]
    for k, b in seed:
        s3.put_object(Bucket="default", Key=k, Body=b, ContentType="text/plain")
    check("boto put 4 keys", True)

    o = s3.head_object(Bucket="default", Key="boto/a.txt")
    check("boto head size/ETag/LastModified",
          o["ContentLength"] == 2 and o["ETag"].strip('"') != "" and "LastModified" in o,
          f"size={o['ContentLength']} etag={o['ETag']}")

    r = s3.list_objects_v2(Bucket="default", Prefix="boto/", Delimiter="/")
    keys = [c["Key"] for c in r.get("Contents", [])]
    cps = [p["Prefix"] for p in r.get("CommonPrefixes", [])]
    check("boto list_objects_v2 delimiter",
          cps == ["boto/dir/"] and sorted(keys) == ["boto/a.txt", "boto/b.txt", "boto/range.bin"],
          f"keys={keys} cps={cps}")

    pg = s3.get_paginator("list_objects_v2")
    pages = list(pg.paginate(Bucket="default", Prefix="boto/", PaginationConfig={"PageSize": 1}))
    allkeys = [c["Key"] for p in pages for c in p.get("Contents", [])]
    check("boto paginator full+no dup",
          len(allkeys) == len(set(allkeys)) and len(allkeys) == 4, f"pages={len(pages)} keys={allkeys}")

    r1 = s3.list_objects(Bucket="default", Prefix="boto/", MaxKeys=2)
    check("boto list_objects V1",
          len(r1.get("Contents", [])) == 2 and r1["IsTruncated"] is True,
          f"trunc={r1.get('IsTruncated')}")

    g = s3.get_object(Bucket="default", Key="boto/range.bin", Range="bytes=2-5")
    check("boto Range 206 body+ContentRange",
          g["Body"].read() == blob[2:6] and g["ContentRange"] == "bytes 2-5/16", g.get("ContentRange"))

    try:
        s3.get_object(Bucket="default", Key="boto/range.bin", Range="bytes=999999-")
        check("boto Range 416", False, "no error raised")
    except ClientError as e:
        check("boto Range 416",
              e.response["ResponseMetadata"]["HTTPStatusCode"] == 416,
              e.response["Error"]["Code"])

    for k, _ in seed:
        s3.delete_object(Bucket="default", Key=k)
    check("boto delete", True)

    # ── multipart（真实客户端大文件路径）──
    key = "boto-mp/big.bin"
    chunks = [bytes([i]) * (1024 * 1024) for i in range(1, 5)]
    whole = b"".join(chunks)
    mpu = s3.create_multipart_upload(Bucket="default", Key=key, ContentType="application/octet-stream")
    uid = mpu["UploadId"]
    check("boto mpu create", bool(uid), uid)
    parts = []
    for i, c in enumerate(chunks, start=1):
        r = s3.upload_part(Bucket="default", Key=key, PartNumber=i, UploadId=uid, Body=c)
        parts.append({"ETag": r["ETag"], "PartNumber": i})
    check("boto mpu upload 4 parts", len(parts) == 4 and all(p["ETag"] for p in parts), parts[0]["ETag"])
    lp = s3.list_parts(Bucket="default", Key=key, UploadId=uid)
    check("boto mpu list_parts", len(lp.get("Parts", [])) == 4, [p["PartNumber"] for p in lp.get("Parts", [])])
    got = {p["PartNumber"]: p.get("ETag") for p in lp.get("Parts", [])}
    want = {p["PartNumber"]: p["ETag"] for p in parts}
    check("boto mpu list_parts etags match upload_part", got == want, got)
    bad = [dict(p) for p in parts]
    bad[0]["ETag"] = '"deadbeefdeadbeefdeadbeefdeadbeef"'
    try:
        s3.complete_multipart_upload(Bucket="default", Key=key, UploadId=uid,
                                     MultipartUpload={"Parts": bad})
        wrong_rejected = False
    except ClientError:
        wrong_rejected = True
    check("boto mpu complete wrong etag rejected", wrong_rejected)
    s3.complete_multipart_upload(Bucket="default", Key=key, UploadId=uid, MultipartUpload={"Parts": parts})
    g = s3.get_object(Bucket="default", Key=key)
    body = g["Body"].read()
    check("boto mpu complete assembles bytes", body == whole, f"{len(body)} vs {len(whole)}")
    mpu2 = s3.create_multipart_upload(Bucket="default", Key="boto-mp/abort.bin")
    uid2 = mpu2["UploadId"]
    s3.upload_part(Bucket="default", Key="boto-mp/abort.bin", PartNumber=1, UploadId=uid2, Body=b"x" * 100)
    s3.abort_multipart_upload(Bucket="default", Key="boto-mp/abort.bin", UploadId=uid2)
    check("boto mpu abort", True)
    s3.delete_object(Bucket="default", Key=key)

    # ── 流式 PUT（8 MiB）+ 批量删（r6）──
    import os

    blob = os.urandom(8 * 1024 * 1024)
    r = s3.put_object(Bucket="default", Key="boto-r6/big.bin", Body=blob,
                      ContentType="application/octet-stream")
    check("boto streaming PUT returns ETag", r.get("ETag") is not None, r.get("ETag"))
    g = s3.get_object(Bucket="default", Key="boto-r6/big.bin")
    body = g["Body"].read()
    check("boto streaming PUT bytes", body == blob, f"{len(body)} vs {len(blob)}")

    for k in ["boto-r6/a.txt", "boto-r6/b.txt"]:
        s3.put_object(Bucket="default", Key=k, Body=b"x")
    resp = s3.delete_objects(Bucket="default", Delete={
        "Objects": [{"Key": "boto-r6/a.txt"}, {"Key": "boto-r6/b.txt"}, {"Key": "boto-r6/missing.txt"}],
        "Quiet": False,
    })
    check("boto DeleteObjects reports all deleted",
          len(resp.get("Deleted", [])) == 3 and len(resp.get("Errors", [])) == 0,
          [x["Key"] for x in resp.get("Deleted", [])])
    lst = s3.list_objects_v2(Bucket="default", Prefix="boto-r6/a")
    check("boto DeleteObjects removed keys", len(lst.get("Contents", [])) == 0)
    s3.put_object(Bucket="default", Key="boto-r6/q.txt", Body=b"y")
    rq = s3.delete_objects(Bucket="default", Delete={
        "Objects": [{"Key": "boto-r6/q.txt"}], "Quiet": True})
    check("boto DeleteObjects Quiet suppresses entries", len(rq.get("Deleted", [])) == 0)
    s3.delete_object(Bucket="default", Key="boto-r6/big.bin")

    # ── 服务端复制 + 第二凭证（r10）──
    cblob = b"copy-src-bytes" * 40
    s3.put_object(Bucket="default", Key="boto-r10/src.bin", Body=cblob,
                  ContentType="application/x-thing")
    r = s3.copy_object(Bucket="default", Key="boto-r10/dst.bin",
                       CopySource="default/boto-r10/src.bin")
    g = s3.get_object(Bucket="default", Key="boto-r10/dst.bin")
    check("boto CopyObject bytes + ContentType",
          g["Body"].read() == cblob and g.get("ContentType") == "application/x-thing",
          f"{g.get('ContentType')} etag={r.get('CopyObjectResult', {}).get('ETag')}")
    s3.delete_object(Bucket="default", Key="boto-r10/src.bin")
    s3.delete_object(Bucket="default", Key="boto-r10/dst.bin")
    if ACCESS2:
        s3b = boto3.client(
            "s3", endpoint_url=ENDPOINT, aws_access_key_id=ACCESS2,
            aws_secret_access_key=SECRET2, region_name="us-east-1",
            config=Config(s3={"addressing_style": "path"}, retries={"max_attempts": 1}))
        if NS_ACCESS and NS_SECRET:
            writer = boto3.client(
                "s3", endpoint_url=ENDPOINT, aws_access_key_id=NS_ACCESS,
                aws_secret_access_key=NS_SECRET, region_name="us-east-1",
                config=Config(s3={"addressing_style": "path"}, retries={"max_attempts": 1}))
            probe_key = "boto-ns2/second.txt"
        else:
            writer = s3
            probe_key = "boto-r10/second.txt"
        writer.put_object(Bucket="default", Key=probe_key, Body=b"two")
        read_ok = s3b.get_object(Bucket="default", Key=probe_key)["Body"].read() == b"two"
        try:
            boto3.client(
                "s3", endpoint_url=ENDPOINT, aws_access_key_id="definitely-unknown",
                aws_secret_access_key="x", region_name="us-east-1",
                config=Config(s3={"addressing_style": "path"}, retries={"max_attempts": 1})
            ).list_buckets()
            unknown_rejected = False
        except ClientError:
            unknown_rejected = True
        if SECOND_RO:
            # 只读键（`access:secret:ro`）→ 读可用、写被拒（r21）
            try:
                s3b.put_object(Bucket="default", Key=f"{probe_key}.nope", Body=b"x")
                ro_enforced = False
            except ClientError as e:
                ro_enforced = e.response["Error"]["Code"] in ("AccessDenied", "AccessDeniedException")
            check("boto read-only credential: read ok, write denied",
                  read_ok and ro_enforced and unknown_rejected,
                  f"read={read_ok} denied={ro_enforced} unknown={unknown_rejected}")
        else:
            s3b.put_object(Bucket="default", Key=probe_key, Body=b"two")
            write_ok = s3b.get_object(Bucket="default", Key=probe_key)["Body"].read() == b"two"
            check("boto second credential works + unknown rejected",
                  read_ok and write_ok and unknown_rejected,
                  f"read={read_ok} write={write_ok} unknown={unknown_rejected}")
            s3b.delete_object(Bucket="default", Key=probe_key)
        writer.delete_object(Bucket="default", Key=probe_key)

    # ── 大桶分页（一条 SQL 取全 ✗ r13）──
    for k in range(4):
        for j in range(30):
            s3.put_object(Bucket="default", Key=f"boto-scale/d{k}/f{j:02d}.txt", Body=b"x")
    pages = list(s3.get_paginator("list_objects_v2").paginate(
        Bucket="default", Prefix="boto-scale/", PaginationConfig={"PageSize": 50}))
    skeys = [c["Key"] for p in pages for c in p.get("Contents", [])]
    check("boto scale pagination 120 no dup",
          len(skeys) == 120 and len(set(skeys)) == 120, f"pages={len(pages)} keys={len(skeys)}")
    sc = s3.list_objects_v2(Bucket="default", Prefix="boto-scale/", Delimiter="/")
    scps = sorted(p["Prefix"] for p in sc.get("CommonPrefixes", []))
    check("boto scale delimiter 4 prefixes", len(scps) == 4, scps)
    # V1 marker 分页 + StartAfter + 单键续页走全（r20 SQL 逐页）
    v1 = s3.list_objects(Bucket="default", Prefix="boto-scale/", MaxKeys=50)
    v1b = s3.list_objects(Bucket="default", Prefix="boto-scale/", MaxKeys=50,
                          Marker=v1.get("NextMarker", ""))
    check("boto V1 marker paging", v1["IsTruncated"] and len(v1b.get("Contents", [])) == 50,
          v1.get("NextMarker"))
    sa = s3.list_objects_v2(Bucket="default", Prefix="boto-scale/",
                            StartAfter="boto-scale/d1/f29.txt", MaxKeys=5)
    check("boto StartAfter", all(c["Key"] > "boto-scale/d1/f29.txt"
                                for c in sa.get("Contents", [])),
          [c["Key"] for c in sa.get("Contents", [])])
    n, tok, guard = 0, None, 0
    while guard < 500:
        guard += 1
        r = (s3.list_objects_v2(Bucket="default", Prefix="boto-scale/", MaxKeys=1,
                                ContinuationToken=tok) if tok
             else s3.list_objects_v2(Bucket="default", Prefix="boto-scale/", MaxKeys=1))
        n += len(r.get("Contents", []))
        if not r["IsTruncated"]:
            break
        tok = r["NextContinuationToken"]
    check("boto single-key continuation walks all", n == 120, f"{n} keys in {guard} pages")
    for k in range(4):
        for j in range(30):
            s3.delete_object(Bucket="default", Key=f"boto-scale/d{k}/f{j:02d}.txt")

    # ── 用户元数据（x-amz-meta-* ✗ r22）──
    umd = {"project": "alpha", "owner": "ops", "x-custom": "v=1"}
    s3.put_object(Bucket="default", Key="boto-meta/a.txt", Body=b"hello", Metadata=umd)
    check("boto user metadata round-trip (GET)",
          s3.get_object(Bucket="default", Key="boto-meta/a.txt")["Metadata"] == umd)
    check("boto user metadata round-trip (HEAD)",
          s3.head_object(Bucket="default", Key="boto-meta/a.txt")["Metadata"] == umd)
    s3.put_object(Bucket="default", Key="boto-meta/a.txt", Body=b"again")
    check("boto overwrite clears metadata",
          s3.head_object(Bucket="default", Key="boto-meta/a.txt")["Metadata"] == {})
    s3.put_object(Bucket="default", Key="boto-meta/src.txt", Body=b"s", Metadata=umd)
    s3.copy_object(Bucket="default", Key="boto-meta/copy.txt", CopySource="default/boto-meta/src.txt")
    check("boto copy preserves metadata",
          s3.head_object(Bucket="default", Key="boto-meta/copy.txt")["Metadata"] == umd)
    s3.copy_object(Bucket="default", Key="boto-meta/repl.txt", CopySource="default/boto-meta/src.txt",
                   MetadataDirective="REPLACE", Metadata={"only": "one"})
    check("boto copy REPLACE metadata",
          s3.head_object(Bucket="default", Key="boto-meta/repl.txt")["Metadata"] == {"only": "one"})
    for mk in ["boto-meta/a.txt", "boto-meta/src.txt", "boto-meta/copy.txt", "boto-meta/repl.txt"]:
        s3.delete_object(Bucket="default", Key=mk)

    # ── 批量删的逐键条件（ETag ✗ r29）──
    for bk in ["boto-dc/a", "boto-dc/b"]:
        s3.put_object(Bucket="default", Key=bk, Body=b"x")
    be = s3.head_object(Bucket="default", Key="boto-dc/b")["ETag"]
    br = s3.delete_objects(Bucket="default", Delete={"Objects": [
        {"Key": "boto-dc/a", "ETag": '"deadbeefdeadbeefdeadbeefdeadbeef"'},
        {"Key": "boto-dc/b", "ETag": be},
    ]})
    bdel = sorted(d["Key"] for d in br.get("Deleted", []))
    berr = {e["Key"]: e["Code"] for e in br.get("Errors", [])}
    check("boto DeleteObjects per-key etag condition",
          bdel == ["boto-dc/b"] and berr == {"boto-dc/a": "PreconditionFailed"},
          f"deleted={bdel} errors={berr}")
    s3.delete_object(Bucket="default", Key="boto-dc/a")

    # ── 条件写 / 条件删（If-Match / If-None-Match ✗ r28）──
    def cw(fn):
        try:
            fn()
            return "ok"
        except ClientError as e:
            return e.response["Error"]["Code"]

    ck = "boto-condw/a.txt"
    n1 = cw(lambda: s3.put_object(Bucket="default", Key=ck, Body=b"one", IfNoneMatch="*")) == "ok"
    n2 = cw(lambda: s3.put_object(Bucket="default", Key=ck, Body=b"x", IfNoneMatch="*")) == "PreconditionFailed"
    ce = s3.head_object(Bucket="default", Key=ck)["ETag"]
    m1 = cw(lambda: s3.put_object(Bucket="default", Key=ck, Body=b"two", IfMatch=ce)) == "ok"
    m2 = cw(lambda: s3.put_object(Bucket="default", Key=ck, Body=b"x", IfMatch=ce)) == "PreconditionFailed"
    d1 = cw(lambda: s3.delete_object(Bucket="default", Key=ck, IfMatch=ce)) == "PreconditionFailed"
    d2 = cw(lambda: s3.delete_object(Bucket="default", Key=ck,
                                     IfMatch=s3.head_object(Bucket="default", Key=ck)["ETag"])) == "ok"
    check("boto conditional write/delete (If-Match / If-None-Match)",
          n1 and n2 and m1 and m2 and d1 and d2,
          f"{n1}/{n2}/{m1}/{m2}/{d1}/{d2}")

    # ── 凭证→命名空间绑定（多租户隔离 ✗ r26）──
    if NS_ACCESS and NS_SECRET:
        nsc = boto3.client(
            "s3", endpoint_url=ENDPOINT, aws_access_key_id=NS_ACCESS,
            aws_secret_access_key=NS_SECRET, region_name="us-east-1",
            config=Config(s3={"addressing_style": "path"}, retries={"max_attempts": 1}))
        s3.put_object(Bucket="default", Key="boto-ns/base.txt", Body=b"base")
        nsc.put_object(Bucket="default", Key="boto-ns/tenant.txt", Body=b"tenant")
        bl = sorted(o["Key"] for o in s3.list_objects_v2(
            Bucket="default", Prefix="boto-ns/").get("Contents", []))
        tl = sorted(o["Key"] for o in nsc.list_objects_v2(
            Bucket="default", Prefix="boto-ns/").get("Contents", []))
        iso = bl == ["boto-ns/base.txt"] and tl == ["boto-ns/tenant.txt"]
        try:
            nsc.get_object(Bucket="default", Key="boto-ns/base.txt")
            cross = False
        except ClientError as e:
            cross = e.response["Error"]["Code"] == "NoSuchKey"
        check("boto credential→namespace isolation", iso and cross,
              f"base={bl} tenant={tl} cross404={cross}")
        s3.delete_object(Bucket="default", Key="boto-ns/base.txt")
        nsc.delete_object(Bucket="default", Key="boto-ns/tenant.txt")

    # ── 条件复制（x-amz-copy-source-if-* ✗ r24）──
    s3.put_object(Bucket="default", Key="boto-cond/src.txt", Body=b"c")
    cetag = s3.head_object(Bucket="default", Key="boto-cond/src.txt")["ETag"]

    def ctry(**kw):
        try:
            s3.copy_object(Bucket="default", Key="boto-cond/dst.txt",
                           CopySource="default/boto-cond/src.txt", **kw)
            return "ok"
        except ClientError as e:
            return e.response["Error"]["Code"]

    cok = ctry(CopySourceIfMatch=cetag) == "ok"
    cbad = ctry(CopySourceIfMatch='"deadbeef"') == "PreconditionFailed"
    cnone = ctry(CopySourceIfNoneMatch=cetag) == "PreconditionFailed"
    check("boto conditional copy (if-match / if-none-match)",
          cok and cbad and cnone, f"ok={cok} bad={cbad} none={cnone}")
    s3.delete_object(Bucket="default", Key="boto-cond/src.txt")
    s3.delete_object(Bucket="default", Key="boto-cond/dst.txt")

    # ── multipart 用户元数据（CreateMultipartUpload 传入 ✗ r23）──
    mmd = {"project": "big", "stage": "mpu"}
    muid = s3.create_multipart_upload(Bucket="default", Key="boto-mpu/meta.bin",
                                      Metadata=mmd)["UploadId"]
    mparts = []
    for i, c in enumerate([b"a" * 1024, b"b" * 1024], 1):
        r = s3.upload_part(Bucket="default", Key="boto-mpu/meta.bin", PartNumber=i,
                           UploadId=muid, Body=c)
        mparts.append({"ETag": r["ETag"], "PartNumber": i})
    s3.complete_multipart_upload(Bucket="default", Key="boto-mpu/meta.bin", UploadId=muid,
                                 MultipartUpload={"Parts": mparts})
    check("boto multipart metadata applied on complete",
          s3.head_object(Bucket="default", Key="boto-mpu/meta.bin")["Metadata"] == mmd)
    s3.delete_object(Bucket="default", Key="boto-mpu/meta.bin")

    # ── UploadPartCopy（r19）──
    cpsrc = bytes([i % 251 for i in range(262144)])
    s3.put_object(Bucket="default", Key="boto-upc/src.bin", Body=cpsrc)
    cuid = s3.create_multipart_upload(Bucket="default", Key="boto-upc/dst.bin")["UploadId"]
    c1 = s3.upload_part_copy(Bucket="default", Key="boto-upc/dst.bin", PartNumber=1,
                             UploadId=cuid, CopySource="default/boto-upc/src.bin",
                             CopySourceRange="bytes=0-131071")["CopyPartResult"]["ETag"]
    c2 = s3.upload_part_copy(Bucket="default", Key="boto-upc/dst.bin", PartNumber=2,
                             UploadId=cuid, CopySource="default/boto-upc/src.bin",
                             CopySourceRange="bytes=131072-")["CopyPartResult"]["ETag"]
    s3.complete_multipart_upload(Bucket="default", Key="boto-upc/dst.bin", UploadId=cuid,
                                MultipartUpload={"Parts": [{"ETag": c1, "PartNumber": 1},
                                                           {"ETag": c2, "PartNumber": 2}]})
    check("boto upload_part_copy range == source",
          s3.get_object(Bucket="default", Key="boto-upc/dst.bin")["Body"].read() == cpsrc)
    s3.delete_object(Bucket="default", Key="boto-upc/src.bin")
    s3.delete_object(Bucket="default", Key="boto-upc/dst.bin")

    # ── ListMultipartUploads（r16）──
    muids = {}
    for mk in ["boto-lmu/x.bin", "boto-lmu/sub/y.bin"]:
        muids[mk] = s3.create_multipart_upload(Bucket="default", Key=mk)["UploadId"]
    lm = s3.list_multipart_uploads(Bucket="default", Prefix="boto-lmu/")
    check("boto list_multipart_uploads",
          sorted(u["Key"] for u in lm.get("Uploads", [])) == sorted(muids),
          [u["Key"] for u in lm.get("Uploads", [])])
    lmd = s3.list_multipart_uploads(Bucket="default", Prefix="boto-lmu/", Delimiter="/")
    check("boto list_multipart_uploads delimiter",
          [c["Prefix"] for c in lmd.get("CommonPrefixes", [])] == ["boto-lmu/sub/"],
          lmd.get("CommonPrefixes"))
    for mk, mid in muids.items():
        s3.abort_multipart_upload(Bucket="default", Key=mk, UploadId=mid)

    passed = sum(1 for x in P if x)
    print(f"== boto3 {passed}/{len(P)} PASS ==")
    sys.exit(0 if passed == len(P) else 1)

if __name__ == "__main__":
    main()
