# Protocol compatibility audit

Use this register to turn protocol review into a repeatable loop: discover a
concrete issue, record its evidence and priority, fix one item, run the relevant
checks, and record the commit that closes it.

## Findings

| ID | Protocol | Finding and evidence | Priority | State |
| --- | --- | --- | --- | --- |
| HTTP-001 | HTTP | CORS previously allowed only `Accept` and `Content-Type`, blocking browser preflight for Bearer auth and conditional/range downloads; successful responses also hid validators and range metadata from browser JavaScript. Explicit request and exposed response headers plus preflight/206 regression coverage are now in place. | P1 | Fixed in this cycle |
| HTTP-002 | HTTP | Search queries joined every historical version, duplicating filename matches and allowing obsolete overwritten content to match. Both joins now select only the maximum version; a replacement-upload regression test covers unique results and current-content semantics. | P1 | Fixed in this cycle |
| DAV-001 | WebDAV | `Overwrite` was parsed case-sensitively. RFC 4918 defines `T`/`F` as ABNF string literals, which are case-insensitive; a compliant lowercase `f` was rejected. Parser and request-level regression coverage added. | P1 | Fixed: `b8accb2` |
| S3-001 | S3 | `DeleteObjects.LastModifiedTime` was compared with immutable entry creation time, so a correct condition from `HeadObject` failed after overwriting an object. It now batches current version lookup and compares against that version timestamp; boto3 verifies stale time rejection and current time acceptance after overwrite. | P1 | Fixed in this cycle |
| QA-001 | S3 | Independent-client probe: boto3/botocore 1.34.46 passed 51/51 operations; rclone 1.60.1-DEV passed bucket listing, upload, list, download, 11 MiB multipart upload/download byte comparison, delete, Unicode/space/empty keys, checksum check, and upload/list/sync of 1005 objects across the 1000-key page boundary. | P1 | Verified for tested operations |
| QA-002 | WebDAV | cadaver 0.24 passed authenticated OPTIONS, MKCOL, 11 MiB PUT, COPY, MOVE, PROPFIND listing, GET byte comparison, DELETE, and collection removal. rclone 1.60.1-DEV also passed MKCOL/PUT/list/GET. | P1 | Verified for tested operations; full litmus suite remains open |
| QA-003 | RSYNC | System rsync 3.2.7 protocol 31 passed live against an isolated daemon: module discovery, recursive push/pull with empty directories and a 2 MiB file, checksum incremental update (32 literal bytes), `--delete --exclude` protection, and `--delete-excluded`. | P1 | Live client verified; automated real-client harness remains open |
| QA-004 | S3 test tooling | The installed botocore 1.34.46 S3 model predates conditional `DeleteObjects` fields and `PutObject`/`DeleteObject` conditions used by the probe. The probe now supplies those missing model members, including RFC 822 timestamp serialization; the complete 50/50 run passes. | P1 | Fixed in this cycle |

## Discovery and verification baseline

- Static analysis: `cargo clippy --workspace --all-targets -- -D warnings`.
- Formatting gate: `cargo fmt --all -- --check`.
- Regression baseline: `cargo test --workspace` plus protocol-specific end-to-end tests.
- Third-party tools found in the current development environment: `curl` 8.5.0,
  `rsync` 3.2.7 (protocol 31), `rclone` 1.60.1-DEV, `cadaver` 0.24,
  `boto3` 1.34.46, and `botocore` 1.34.46. `litmus`, `aws`, and `s3cmd` were
  not found.
- Manual client checks in this environment are evidence for the operations
  listed above, not proof of complete protocol conformance. Automate these
  scenarios and add real rsync daemon client coverage to CI as follow-up work.

This is an environment snapshot, not a claim that the listed protocols are
fully conformant. Recheck tool availability and update this register when the
environment or test harness changes.
