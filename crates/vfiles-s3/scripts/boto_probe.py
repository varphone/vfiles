#!/usr/bin/env python3
"""S3 真客户端实证（可选第二入口 ✗ 需 `pip install boto3` = 官方 AWS SDK 独立校验）。

与 `sigv4_probe.py` 的自写 SigV4 形成双通道：本脚本由 botocore 负责签名 / XML 解析 /
分页器 / Range，验证我们的响应对真实 SDK 的兼容性（含 SDK 驱动的 paginator 无重无漏）。

用法：起服后 `python3 crates/vfiles-s3/scripts/boto_probe.py http://127.0.0.1:9000`
"""
import sys
import datetime
import os
import time
from urllib.parse import unquote
from urllib.error import HTTPError
from urllib.request import urlopen

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
        config=Config(signature_version="s3v4", s3={"addressing_style": "path"},
                      retries={"max_attempts": 1}),
    )

    # Conditional DeleteObjects fields were added to AWS's S3 model after the
    # botocore version shipped by some distributions. Keep this probe usable
    # with those older SDKs while emitting the RFC 822 timestamp format used
    # by the S3 REST XML API.
    delete_objects = s3.meta.service_model.operation_model("DeleteObjects")
    object_shape = (
        delete_objects.input_shape.members["Delete"]
        .members["Objects"].member
    )
    service_model = s3.meta.service_model
    for name, shape_name in (
        ("ETag", "ETag"),
        ("LastModifiedTime", "LastModified"),
        ("Size", "ContentLength"),
    ):
        if name not in object_shape.members:
            shape = service_model.shape_for(shape_name)
            if name == "LastModifiedTime":
                shape.serialization["timestampFormat"] = "rfc822"
            object_shape.members[name] = shape
    get_object_shape = service_model.operation_model("GetObject").input_shape
    for operation_name, names in (
        ("PutObject", ("IfMatch", "IfNoneMatch")),
        ("DeleteObject", ("IfMatch",)),
    ):
        input_shape = service_model.operation_model(operation_name).input_shape
        for name in names:
            if name not in input_shape.members:
                input_shape.members[name] = get_object_shape.members[name]

    buckets = [b["Name"] for b in s3.list_buckets()["Buckets"]]
    check("boto ListBuckets", buckets == ["default"], buckets)

    # The shared HTTP listener also supports a conventional path-style S3
    # endpoint. Sign and exercise that exact prefixed path with the real SDK;
    # testing only the root endpoint would miss canonical-path regressions.
    mounted_s3 = boto3.client(
        "s3",
        endpoint_url=ENDPOINT.rstrip("/") + "/s3",
        aws_access_key_id=ACCESS,
        aws_secret_access_key=SECRET,
        region_name="us-east-1",
        config=Config(signature_version="s3v4", s3={"addressing_style": "path"},
                      retries={"max_attempts": 1}),
    )
    mounted_key = "boto-mounted-prefix/probe.txt"
    try:
        mounted_buckets = [b["Name"] for b in mounted_s3.list_buckets()["Buckets"]]
        mounted_s3.put_object(Bucket="default", Key=mounted_key, Body=b"signed /s3 path")
        mounted_body = mounted_s3.get_object(Bucket="default", Key=mounted_key)["Body"].read()
        mounted_s3.delete_object(Bucket="default", Key=mounted_key)
        mounted_ok = mounted_buckets == ["default"] and mounted_body == b"signed /s3 path"
    except ClientError as error:
        mounted_ok = False
        mounted_body = error.response.get("Error", {}).get("Code")
    check("boto SigV4 path-style /s3 endpoint", mounted_ok, mounted_body)

    if os.environ.get("VFILES_S3_REGION"):
        wrong_region_client = boto3.client(
            "s3", endpoint_url=ENDPOINT, aws_access_key_id=ACCESS,
            aws_secret_access_key=SECRET, region_name="us-west-2",
            config=Config(signature_version="s3v4", s3={"addressing_style": "path"},
                          retries={"max_attempts": 1}),
        )
        wrong_region_url = wrong_region_client.generate_presigned_url(
            "list_objects_v2", Params={"Bucket": "default"}, ExpiresIn=60
        )
        try:
            wrong_region_response = urlopen(wrong_region_url, timeout=10)
            wrong_region_status = wrong_region_response.status
            wrong_region_body = wrong_region_response.read().decode("utf-8", "replace")
        except HTTPError as error:
            wrong_region_status = error.code
            wrong_region_body = error.read().decode("utf-8", "replace")
        check(
            "boto presigned URL rejects mismatched signing region",
            wrong_region_status == 400 and "AuthorizationHeaderMalformed" in wrong_region_body,
            f"status={wrong_region_status} body={wrong_region_body[:240]}",
        )
        correct_region_url = s3.generate_presigned_url(
            "list_objects_v2", Params={"Bucket": "default"}, ExpiresIn=60
        )
        try:
            with urlopen(correct_region_url, timeout=10) as response:
                check("boto presigned URL accepts configured signing region", response.status == 200)
        except HTTPError as error:
            check("boto presigned URL accepts configured signing region", False,
                  f"status={error.code} body={error.read().decode('utf-8', 'replace')[:240]}")
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
          hb and hb404 and loc == "us-east-1" and ver.get("Status") == "Enabled",
          f"head={hb} 404={hb404} loc={loc} ver={ver.get('Status')}")

    blob = b"0123456789abcdef"
    seed = [("boto/a.txt", b"aa"), ("boto/b.txt", b"bb"),
            ("boto/dir/x.txt", b"xx"), ("boto/range.bin", blob)]
    for k, b in seed:
        s3.put_object(Bucket="default", Key=k, Body=b, ContentType="text/plain")
    check("boto put 4 keys", True)

    # S3 has a flat key namespace: leading/trailing slash keys remain distinct
    # objects and prefix filtering applies to their exact, unmodified spelling.
    identity_keys = {
        "boto-key-identity/a": b"plain",
        "/boto-key-identity/a": b"leading-slash",
        "boto-key-identity/a/": b"trailing-slash",
    }
    for key, body in identity_keys.items():
        s3.put_object(Bucket="default", Key=key, Body=body)
    identity_reads = {
        key: s3.get_object(Bucket="default", Key=key)["Body"].read()
        for key in identity_keys
    }
    copied_plain_key = "boto-key-copy/from-trailing-slash"
    copied_slash_key = "/boto-key-copy/to-leading-slash/"
    s3.copy_object(
        Bucket="default",
        Key=copied_plain_key,
        CopySource={"Bucket": "default", "Key": "boto-key-identity/a/"},
    )
    s3.copy_object(
        Bucket="default",
        Key=copied_slash_key,
        CopySource={"Bucket": "default", "Key": "boto-key-identity/a"},
    )
    copy_reads = {
        copied_plain_key: s3.get_object(Bucket="default", Key=copied_plain_key)["Body"].read(),
        copied_slash_key: s3.get_object(Bucket="default", Key=copied_slash_key)["Body"].read(),
    }
    identity_list = s3.list_objects_v2(
        Bucket="default", Prefix="boto-key-identity/"
    ).get("Contents", [])
    identity_list_keys = [item["Key"] for item in identity_list]
    identity_all_keys = {
        item["Key"]
        for item in s3.list_objects_v2(Bucket="default").get("Contents", [])
    }
    identity_trailing = s3.list_objects_v2(
        Bucket="default", Prefix="boto-key-identity/a/"
    ).get("Contents", [])
    check(
        "boto preserves leading/trailing slash key identity and exact prefixes",
        identity_reads == identity_keys
        and copy_reads == {
            copied_plain_key: b"trailing-slash",
            copied_slash_key: b"plain",
        }
        and set(identity_keys).issubset(identity_all_keys)
        and identity_list_keys == ["boto-key-identity/a", "boto-key-identity/a/"]
        and [item["Key"] for item in identity_trailing] == ["boto-key-identity/a/"],
        f"reads={identity_reads} copies={copy_reads} all={identity_all_keys} list={identity_list_keys} "
        f"trailing={identity_trailing}",
    )
    multipart_identity_key = "boto-mpu-key-identity/a/"
    multipart_identity_id = s3.create_multipart_upload(
        Bucket="default", Key=multipart_identity_key
    )["UploadId"]
    multipart_identity_list = s3.list_multipart_uploads(
        Bucket="default", Prefix="boto-mpu-key-identity/"
    ).get("Uploads", [])
    check(
        "boto multipart list preserves the raw trailing-slash key",
        any(
            upload["Key"] == multipart_identity_key
            and upload["UploadId"] == multipart_identity_id
            for upload in multipart_identity_list
        ),
        multipart_identity_list,
    )
    s3.abort_multipart_upload(
        Bucket="default", Key=multipart_identity_key, UploadId=multipart_identity_id
    )
    s3.delete_objects(
        Bucket="default",
        Delete={
            "Objects": [
                {"Key": key}
                for key in [*identity_keys, copied_plain_key, copied_slash_key]
            ]
        },
    )

    try:
        s3.put_object(Bucket="default", Key="k" * 1025, Body=b"invalid")
        long_key_result = (False, "request unexpectedly succeeded")
    except ClientError as e:
        error_code = e.response["Error"]["Code"]
        long_key_result = (
            e.response["ResponseMetadata"]["HTTPStatusCode"] == 400
            and error_code in {"InvalidArgument", "KeyTooLongError"},
            f"{e.response['ResponseMetadata']['HTTPStatusCode']} {error_code}",
        )
    check("boto rejects object keys over 1024 UTF-8 bytes with HTTP 400", *long_key_result)

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
    # Every non-final completed part must meet S3's 5 MiB minimum.
    chunks = [bytes([i]) * (5 * 1024 * 1024) for i in range(1, 4)]
    chunks.append(bytes([4]) * (1024 * 1024))
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

    # ── 版本列表（r35）──
    for vb in [b"v1", b"v2"]:
        s3.put_object(Bucket="default", Key="boto-ver/a.txt", Body=vb)
    lv = s3.list_object_versions(Bucket="default", Prefix="boto-ver/")
    vers = lv.get("Versions", [])
    vh = s3.head_object(Bucket="default", Key="boto-ver/a.txt")
    check("boto list_object_versions (newest first, etag matches head)",
          len(vers) == 2 and vers[0]["IsLatest"] is True and vers[0]["ETag"] == vh["ETag"]
          and sum(1 for v in vers if v["IsLatest"]) == 1,
          [(v["IsLatest"], v.get("Size")) for v in vers])
    p1 = s3.list_object_versions(Bucket="default", Prefix="boto-ver/", MaxKeys=1)
    p2 = s3.list_object_versions(Bucket="default", Prefix="boto-ver/", MaxKeys=1,
                                 KeyMarker=p1.get("NextKeyMarker", ""),
                                 VersionIdMarker=p1.get("NextVersionIdMarker", ""))
    paged = [(p1.get("Versions", []) + p2.get("Versions", []))]
    got = paged[0]
    check("boto list_object_versions paging walks all versions",
          len(got) == 2 and len({(v['Key'], v['VersionId']) for v in got}) == 2,
          len(got))
    s3.delete_object(Bucket="default", Key="boto-ver/a.txt")

    # ── versionId 定向读/删 + 版本控制配置（r36）──
    vk = "boto-vid/a.txt"
    for vb in [b"one", b"two", b"three"]:
        s3.put_object(Bucket="default", Key=vk, Body=vb)
        if vb != b"three":
            time.sleep(1.1)  # Make stale entry-created timestamps observable at HTTP-date precision.
    vl = s3.list_object_versions(Bucket="default", Prefix="boto-vid/")["Versions"]
    oldv = [v for v in vl if not v["IsLatest"]][0]
    g = s3.get_object(Bucket="default", Key=vk, VersionId=oldv["VersionId"])
    curv = [v for v in vl if v["IsLatest"]][0]
    current_head = s3.head_object(Bucket="default", Key=vk)
    vid_ok = (
        len(vl) == 3
        and g["ETag"] == oldv["ETag"]
        and g.get("VersionId") == oldv["VersionId"]
        and g["Body"].read() == b"two"
        and int(g["LastModified"].timestamp()) == int(oldv["LastModified"].timestamp())
        and curv.get("VersionId") == current_head.get("VersionId")
        and curv.get("ETag") == current_head.get("ETag")
        and int(curv["LastModified"].timestamp()) == int(current_head["LastModified"].timestamp())
    )
    def vcode(fn):
        try:
            fn()
            return "ok"
        except ClientError as e:
            return e.response["Error"]["Code"]
    oldv1 = [v for v in vl if not v["IsLatest"] and v["VersionId"] != oldv["VersionId"]][0]
    del_ok = vcode(lambda: s3.delete_object(Bucket="default", Key=vk,
                                            VersionId=oldv1["VersionId"])) == "ok"
    del_cur = vcode(lambda: s3.delete_object(Bucket="default", Key=vk,
                                             VersionId=curv["VersionId"])) == "ok"
    revealed = s3.get_object(Bucket="default", Key=vk)["Body"].read() == b"two"
    bad_ok = vcode(lambda: s3.get_object(Bucket="default", Key=vk, VersionId="deadbeef")) == "NoSuchVersion"
    vcfg = s3.get_bucket_versioning(Bucket="default").get("Status") == "Enabled"
    put_ok = vcode(lambda: s3.put_bucket_versioning(
        Bucket="default", VersioningConfiguration={"Status": "Enabled"})) == "ok"
    sus = vcode(lambda: s3.put_bucket_versioning(
        Bucket="default", VersioningConfiguration={"Status": "Suspended"})) == "InvalidArgument"
    check("boto versionId targeting + versioning config",
          vid_ok and del_ok and del_cur and revealed and bad_ok and vcfg and put_ok and sus,
          f"vid={vid_ok} del={del_ok} cur={del_cur} revealed={revealed} bad={bad_ok} cfg={vcfg} put={put_ok} sus={sus}")
    marker_delete = s3.delete_object(Bucket="default", Key=vk)
    marker_id = marker_delete.get("VersionId")
    marker_hidden = vcode(lambda: s3.get_object(Bucket="default", Key=vk)) == "NoSuchKey"
    marker_copy_hidden = marker_latest = marker_read = marker_restored = False
    if marker_id:
        marker_copy_hidden = vcode(lambda: s3.copy_object(
            Bucket="default", Key="boto-vid/copy-of-deleted.txt",
            CopySource=f"default/{vk}")) == "NoSuchKey"
        latest = s3.list_object_versions(Bucket="default", Prefix=vk)
        marker_latest = any(
            marker.get("VersionId") == marker_id and marker.get("IsLatest")
            for marker in latest.get("DeleteMarkers", [])
        )
        try:
            s3.get_object(Bucket="default", Key=vk, VersionId=marker_id)
        except ClientError as e:
            marker_read = (
                e.response["ResponseMetadata"]["HTTPStatusCode"] == 405
                and e.response["ResponseMetadata"]["HTTPHeaders"].get("x-amz-delete-marker") == "true"
            )
        s3.delete_object(Bucket="default", Key=vk, VersionId=marker_id)
        marker_restored = s3.get_object(Bucket="default", Key=vk)["Body"].read() == b"two"
    check("boto delete marker list/read/unmark semantics",
          bool(marker_id) and marker_latest and marker_hidden and marker_copy_hidden
          and marker_read and marker_restored,
          f"id={bool(marker_id)} latest={marker_latest} hidden={marker_hidden} copy_hidden={marker_copy_hidden} read={marker_read} restored={marker_restored}")

    # Current delete markers must be applied before delimiter folding and
    # pagination: hidden-only prefixes disappear, and pages contain visible
    # keys instead of empty pages that only advanced past hidden objects.
    hidden_dir_key = "boto-list-markers/deleted/only.txt"
    page_hidden_key = "boto-list-marker-page/00-hidden.txt"
    visible_page_keys = [
        "boto-list-marker-page/01-visible.txt",
        "boto-list-marker-page/02-visible.txt",
    ]
    for key in [hidden_dir_key, page_hidden_key, *visible_page_keys,
                "boto-list-markers/live/visible.txt"]:
        s3.put_object(Bucket="default", Key=key, Body=b"visible-list-fixture")
    s3.delete_object(Bucket="default", Key=hidden_dir_key)
    s3.delete_object(Bucket="default", Key=page_hidden_key)
    folded = s3.list_objects_v2(
        Bucket="default", Prefix="boto-list-markers/", Delimiter="/"
    )
    folded_prefixes = sorted(p["Prefix"] for p in folded.get("CommonPrefixes", []))
    folded_ok = folded_prefixes == ["boto-list-markers/live/"]
    first_marker_page = s3.list_objects_v2(
        Bucket="default", Prefix="boto-list-marker-page/", MaxKeys=1
    )
    second_marker_page = s3.list_objects_v2(
        Bucket="default", Prefix="boto-list-marker-page/", MaxKeys=1,
        ContinuationToken=first_marker_page.get("NextContinuationToken", ""),
    )
    marker_pages_ok = (
        [o["Key"] for o in first_marker_page.get("Contents", [])]
        == [visible_page_keys[0]]
        and first_marker_page["IsTruncated"] is True
        and [o["Key"] for o in second_marker_page.get("Contents", [])]
        == [visible_page_keys[1]]
        and second_marker_page["IsTruncated"] is False
    )
    check("boto delete markers before delimiter folding and pagination",
          folded_ok and marker_pages_ok,
          f"prefixes={folded_prefixes} first={first_marker_page.get('Contents')} "
          f"second={second_marker_page.get('Contents')}")

    missing_marker = s3.delete_objects(Bucket="default", Delete={
        "Objects": [{"Key": "boto-vid/never-existed.txt"}], "Quiet": False,
    }).get("Deleted", [{}])[0]
    missing_marker_id = missing_marker.get("DeleteMarkerVersionId")
    listed_missing_marker = any(
        marker.get("Key") == "boto-vid/never-existed.txt"
        and marker.get("VersionId") == missing_marker_id
        and marker.get("IsLatest")
        for marker in s3.list_object_versions(Bucket="default", Prefix="boto-vid/").get("DeleteMarkers", [])
    )
    unmarked = s3.delete_objects(Bucket="default", Delete={
        "Objects": [{"Key": "boto-vid/never-existed.txt", "VersionId": missing_marker_id}],
        "Quiet": False,
    }).get("Deleted", [{}])[0]
    check("boto DeleteObjects missing-key marker create/list/remove",
          missing_marker.get("DeleteMarker") is True and bool(missing_marker_id)
          and listed_missing_marker and unmarked.get("DeleteMarker") is True
          and unmarked.get("VersionId") == missing_marker_id,
          f"create={missing_marker} unmark={unmarked}")
    s3.delete_object(Bucket="default", Key=vk)

    # ── 桶生命周期（r34）──
    def bcode(fn):
        try:
            fn()
            return "ok"
        except ClientError as e:
            return e.response["Error"]["Code"]
    try:
        create_default = s3.create_bucket(Bucket="default")
        create_default_status = create_default.get("ResponseMetadata", {}).get("HTTPStatusCode")
        create_default_location = create_default.get("Location")
        create_default_ok = create_default_status == 200 and create_default_location == "/default"
    except ClientError as e:
        create_default_ok = False
        create_default_status = e.response["ResponseMetadata"].get("HTTPStatusCode")
        create_default_location = e.response["Error"]["Code"]
    check("boto bucket lifecycle (idempotent create/delete)",
          create_default_ok
          and bcode(lambda: s3.create_bucket(Bucket="other-b")) == "InvalidBucketName"
          and bcode(lambda: s3.delete_bucket(Bucket="missing")) == "NoSuchBucket"
          and bcode(lambda: s3.delete_bucket(Bucket="default")) == "BucketNotEmpty",
          f"create_owned={create_default_status}/{create_default_location} "
          f"delete={bcode(lambda: s3.delete_bucket(Bucket='default'))}")

    # ── fetch-owner（V2 按需 / V1 恒带 ✗ r33）──
    s3.put_object(Bucket="default", Key="boto-own/a.txt", Body=b"x")
    plain_own = s3.list_objects_v2(Bucket="default", Prefix="boto-own/")
    with_own = s3.list_objects_v2(Bucket="default", Prefix="boto-own/", FetchOwner=True)
    v1_own = s3.list_objects(Bucket="default", Prefix="boto-own/")
    check("boto fetch-owner (V2 on request, V1 always)",
          all("Owner" not in o for o in plain_own.get("Contents", []))
          and all(o.get("Owner", {}).get("ID") for o in with_own.get("Contents", []))
          and all(o.get("Owner", {}).get("ID") for o in v1_own.get("Contents", [])),
          with_own.get("Contents", [{}])[0].get("Owner"))
    s3.delete_object(Bucket="default", Key="boto-own/a.txt")

    # ── encoding-type=url（r32）──
    for ek in ["boto-enc/a%20b.txt", "boto-enc/space key.txt"]:
        s3.put_object(Bucket="default", Key=ek, Body=b"x")
    eraw = [o["Key"] for o in s3.list_objects_v2(
        Bucket="default", Prefix="boto-enc/", EncodingType="url").get("Contents", [])]
    edec = sorted(unquote(k) for k in eraw)
    check("boto encoding-type=url is reversible",
          edec == ["boto-enc/a%20b.txt", "boto-enc/space key.txt"]
          and any("%25" in k for k in eraw),
          eraw)
    for ek in ["boto-enc/a%20b.txt", "boto-enc/space key.txt"]:
        s3.delete_object(Bucket="default", Key=ek)

    # V2 continuation tokens are opaque even when a delimiter prefix itself
    # contains characters that encoding-type=url must escape.
    delimiter_keys = [
        "boto-enc-page/a space/child.bin",
        "boto-enc-page/m% literal.txt",
        "boto-enc-page/z/child.bin",
    ]
    for ek in delimiter_keys:
        s3.put_object(Bucket="default", Key=ek, Body=b"x")
    delimiter_pages = []
    token = None
    for _ in range(4):
        request = {
            "Bucket": "default", "Prefix": "boto-enc-page/",
            "Delimiter": "/", "EncodingType": "url", "MaxKeys": 1,
        }
        if token is not None:
            request["ContinuationToken"] = token
        page = s3.list_objects_v2(**request)
        delimiter_pages.extend(
            unquote(item.get("Key", item.get("Prefix", "")))
            for item in page.get("Contents", []) + page.get("CommonPrefixes", [])
        )
        if not page.get("IsTruncated"):
            break
        token = page.get("NextContinuationToken")
    check("boto URL-encoded delimiter pagination uses opaque continuation tokens",
          delimiter_pages == [
              "boto-enc-page/a space/", "boto-enc-page/m% literal.txt", "boto-enc-page/z/"
          ] and len(delimiter_pages) == 3,
          f"entries={delimiter_pages} pages={len(delimiter_pages)} token={token}")
    v1_delimiter_pages = []
    marker = None
    for _ in range(4):
        request = {
            "Bucket": "default", "Prefix": "boto-enc-page/",
            "Delimiter": "/", "EncodingType": "url", "MaxKeys": 1,
        }
        if marker is not None:
            request["Marker"] = marker
        page = s3.list_objects(**request)
        v1_delimiter_pages.extend(
            unquote(item.get("Key", item.get("Prefix", "")))
            for item in page.get("Contents", []) + page.get("CommonPrefixes", [])
        )
        if not page.get("IsTruncated"):
            break
        marker = page.get("NextMarker")
    check("boto V1 URL-encoded delimiter pagination resumes from NextMarker",
          v1_delimiter_pages == [
              "boto-enc-page/a space/", "boto-enc-page/m% literal.txt", "boto-enc-page/z/"
          ] and len(v1_delimiter_pages) == 3,
          f"entries={v1_delimiter_pages} pages={len(v1_delimiter_pages)} marker={marker}")
    for ek in delimiter_keys:
        s3.delete_object(Bucket="default", Key=ek)

    # ── 批量删的逐键时间/大小条件（r31）──
    for bk in ["boto-dc2/a", "boto-dc2/b", "boto-dc2/c"]:
        s3.put_object(Bucket="default", Key=bk, Body=b"12345")
    dh = s3.head_object(Bucket="default", Key="boto-dc2/a")
    dr = s3.delete_objects(Bucket="default", Delete={"Objects": [
        {"Key": "boto-dc2/a", "LastModifiedTime": dh["LastModified"], "Size": dh["ContentLength"]},
        {"Key": "boto-dc2/b", "Size": dh["ContentLength"] + 1},
        {"Key": "boto-dc2/c", "LastModifiedTime": dh["LastModified"].replace(year=dh["LastModified"].year - 1)},
    ]})
    ddel = sorted(d["Key"] for d in dr.get("Deleted", []))
    derr = {e["Key"]: e["Code"] for e in dr.get("Errors", [])}
    check("boto DeleteObjects per-key time/size conditions",
          ddel == ["boto-dc2/a"] and derr == {"boto-dc2/b": "PreconditionFailed",
                                             "boto-dc2/c": "PreconditionFailed"},
          f"deleted={ddel} errors={derr}")
    for bk in ["boto-dc2/b", "boto-dc2/c"]:
        s3.delete_object(Bucket="default", Key=bk)

    # LastModifiedTime must use the current version after an overwrite, matching
    # HeadObject rather than the immutable entry creation timestamp.
    conditional_key = "boto-dc2/overwritten"
    s3.put_object(Bucket="default", Key=conditional_key, Body=b"old")
    old_modified = s3.head_object(Bucket="default", Key=conditional_key)["LastModified"]
    time.sleep(1.1)
    s3.put_object(Bucket="default", Key=conditional_key, Body=b"new")
    current_head = s3.head_object(Bucket="default", Key=conditional_key)
    stale_result = s3.delete_objects(Bucket="default", Delete={"Objects": [
        {"Key": conditional_key, "LastModifiedTime": old_modified},
    ]})
    stale_errors = {e["Key"]: e["Code"] for e in stale_result.get("Errors", [])}
    current_result = s3.delete_objects(Bucket="default", Delete={"Objects": [
        {"Key": conditional_key, "LastModifiedTime": current_head["LastModified"]},
    ]})
    check("boto DeleteObjects LastModifiedTime follows current overwritten version",
          stale_errors == {conditional_key: "PreconditionFailed"}
          and [d["Key"] for d in current_result.get("Deleted", [])] == [conditional_key],
          f"stale={stale_errors} current={[d['Key'] for d in current_result.get('Deleted', [])]}")

    # ── 条件读（If-None-Match → 304 ✗ r30）──
    s3.put_object(Bucket="default", Key="boto-getc/a.txt", Body=b"hello")
    gh = s3.head_object(Bucket="default", Key="boto-getc/a.txt")
    getc = 0
    try:
        s3.get_object(Bucket="default", Key="boto-getc/a.txt", IfNoneMatch=gh["ETag"])
    except ClientError as e:
        getc = 1 if e.response["Error"]["Code"] == "304" else 0
    other = s3.get_object(Bucket="default", Key="boto-getc/a.txt",
                          IfNoneMatch='"deadbeef"')["Body"].read() == b"hello"
    unmod = 0
    try:
        s3.get_object(Bucket="default", Key="boto-getc/a.txt",
                      IfModifiedSince=gh["LastModified"] + datetime.timedelta(minutes=5))
    except ClientError as e:
        unmod = 1 if e.response["Error"]["Code"] == "304" else 0
    check("boto conditional GET (304 / 200)", getc == 1 and other and unmod == 1,
          f"304={getc} other={other} unmod={unmod}")
    s3.delete_object(Bucket="default", Key="boto-getc/a.txt")

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
    for i, c in enumerate([b"a" * (5 * 1024 * 1024), b"b" * 1024], 1):
        r = s3.upload_part(Bucket="default", Key="boto-mpu/meta.bin", PartNumber=i,
                           UploadId=muid, Body=c)
        mparts.append({"ETag": r["ETag"], "PartNumber": i})
    s3.complete_multipart_upload(Bucket="default", Key="boto-mpu/meta.bin", UploadId=muid,
                                 MultipartUpload={"Parts": mparts})
    check("boto multipart metadata applied on complete",
          s3.head_object(Bucket="default", Key="boto-mpu/meta.bin")["Metadata"] == mmd)
    s3.delete_object(Bucket="default", Key="boto-mpu/meta.bin")

    # ── UploadPartCopy（r19）──
    cpsrc = bytes(range(256)) * (10 * 1024 * 1024 // 256)
    s3.put_object(Bucket="default", Key="boto-upc/src.bin", Body=cpsrc)
    cuid = s3.create_multipart_upload(Bucket="default", Key="boto-upc/dst.bin")["UploadId"]
    c1 = s3.upload_part_copy(Bucket="default", Key="boto-upc/dst.bin", PartNumber=1,
                             UploadId=cuid, CopySource="default/boto-upc/src.bin",
                             CopySourceRange="bytes=0-5242879")["CopyPartResult"]["ETag"]
    c2 = s3.upload_part_copy(Bucket="default", Key="boto-upc/dst.bin", PartNumber=2,
                             UploadId=cuid, CopySource="default/boto-upc/src.bin",
                             CopySourceRange="bytes=5242880-")["CopyPartResult"]["ETag"]
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
    lmd1 = s3.list_multipart_uploads(
        Bucket="default", Prefix="boto-lmu/", Delimiter="/", MaxUploads=1
    )
    lmd2 = s3.list_multipart_uploads(
        Bucket="default", Prefix="boto-lmu/", Delimiter="/", MaxUploads=1,
        KeyMarker=lmd1.get("NextKeyMarker", ""),
        UploadIdMarker=lmd1.get("NextUploadIdMarker", ""),
    )
    check("boto delimiter listing resumes after common prefix",
          lmd1.get("IsTruncated") is True
          and [c["Prefix"] for c in lmd1.get("CommonPrefixes", [])] == ["boto-lmu/sub/"]
          and [u["Key"] for u in lmd2.get("Uploads", [])] == ["boto-lmu/x.bin"],
          (lmd1, lmd2))
    ordered_key = "boto-lmu-ordered/same-key.bin"
    older_upload = s3.create_multipart_upload(Bucket="default", Key=ordered_key)["UploadId"]
    time.sleep(1.1)
    newer_upload = s3.create_multipart_upload(Bucket="default", Key=ordered_key)["UploadId"]
    ordered_first = s3.list_multipart_uploads(
        Bucket="default", Prefix="boto-lmu-ordered/", MaxUploads=1
    )
    ordered_second = s3.list_multipart_uploads(
        Bucket="default", Prefix="boto-lmu-ordered/", MaxUploads=1,
        KeyMarker=ordered_first.get("NextKeyMarker", ""),
        UploadIdMarker=ordered_first.get("NextUploadIdMarker", ""),
    )
    ordered_uploads = ordered_first.get("Uploads", []) + ordered_second.get("Uploads", [])
    check("boto same-key multipart uploads follow initiation order across markers",
          len(ordered_uploads) == 2
          and [u["UploadId"] for u in ordered_uploads] == [older_upload, newer_upload]
          and ordered_uploads[0]["Initiated"] < ordered_uploads[1]["Initiated"],
          ordered_uploads)
    for upload_id in [older_upload, newer_upload]:
        s3.abort_multipart_upload(Bucket="default", Key=ordered_key, UploadId=upload_id)
    for mk, mid in muids.items():
        s3.abort_multipart_upload(Bucket="default", Key=mk, UploadId=mid)

    legacy_upload_id_file = os.environ.get("VFILES_LEGACY_UPLOAD_ID_FILE")
    if legacy_upload_id_file:
        legacy_upload_id = open(legacy_upload_id_file, encoding="utf-8").read()
        legacy_list = s3.list_multipart_uploads(
            Bucket="default", Prefix="boto-legacy-multipart/"
        ).get("Uploads", [])
        check(
            "boto indexed-list startup backfill preserves legacy uploads",
            len(legacy_list) == 1
            and legacy_list[0]["Key"] == "boto-legacy-multipart/pending.bin"
            and legacy_list[0]["UploadId"] == legacy_upload_id,
            legacy_list,
        )

    passed = sum(1 for x in P if x)
    print(f"== boto3 {passed}/{len(P)} PASS ==")
    sys.exit(0 if passed == len(P) else 1)

if __name__ == "__main__":
    main()
