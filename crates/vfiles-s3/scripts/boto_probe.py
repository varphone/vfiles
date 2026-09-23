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

P = []

def check(name, ok, detail=""):
    P.append(ok)
    print(("PASS " if ok else "FAIL ") + name + (" | " + str(detail) if detail else ""))

def main():
    s3 = boto3.client(
        "s3",
        endpoint_url=ENDPOINT,
        aws_access_key_id="test-access",
        aws_secret_access_key="test-secret-123",
        region_name="us-east-1",
        config=Config(s3={"addressing_style": "path"}, retries={"max_attempts": 1}),
    )

    buckets = [b["Name"] for b in s3.list_buckets()["Buckets"]]
    check("boto ListBuckets", buckets == ["default"], buckets)

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

    passed = sum(1 for x in P if x)
    print(f"== boto3 {passed}/{len(P)} PASS ==")
    sys.exit(0 if passed == len(P) else 1)

if __name__ == "__main__":
    main()
