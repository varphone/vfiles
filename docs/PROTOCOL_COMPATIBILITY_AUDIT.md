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
| DAV-002 | WebDAV | `lockinfo` parsing selected the first lock-scope and accepted extra choices, so malformed input containing both `<exclusive/>` and `<shared/>` could be silently granted as exclusive. Parsing now requires exactly one scope choice and one write lock type; unit regressions cover ambiguity. | P1 | Fixed in this cycle |
| S3-001 | S3 | `DeleteObjects.LastModifiedTime` was compared with immutable entry creation time, so a correct condition from `HeadObject` failed after overwriting an object. It now batches current version lookup and compares against that version timestamp; boto3 verifies stale time rejection and current time acceptance after overwrite. | P1 | Fixed in this cycle |
| S3-002 | S3 | Concurrent same-content uploads could both publish via replacing rename and report `created_blob=true`; if one database commit later failed, its cleanup could remove the blob already referenced by the other committed version. Blob files are now synced before no-clobber hard-link publication, the target directory is synced, and a concurrent streaming-write regression asserts exactly one writer owns cleanup. | P1 | Fixed in this cycle |
| S3-003 | S3 | A process may stop after streaming blob publication but before SQLite records the version. A restart regression closes/reopens SQLite with such an unregistered blob present, then verifies maintenance removes it after the grace period while preserving fresh or referenced blobs. Actual power-loss and filesystem fault injection remain untested. | P1 | Restart recovery path verified; hardware fault injection open |
| S3-004 | S3 | `commit_upload_stream` could return early on a path race or repository error after publishing a new blob, leaving it until delayed maintenance; a failure after creating the file entry could also leave a versionless file. Pre-version failures now remove only resources created by this attempt, and a streaming-upload regression covers a target changed to a directory after session creation. | P1 | Fixed in this cycle |
| S3-005 | S3 | Upload-session metadata completion/read happened after the object version was committed but could still fail the request, prompting retries of an already successful write. Post-commit session state and cleanup are now best-effort; a streaming-reader regression corrupts metadata after session lookup and verifies the committed object returns success. | P1 | Fixed in this cycle |
| S3-006 | S3 | Entry re-read and automatic snapshot creation happened after version commit and could fail the upload response. The entry is now carried forward from pre-commit state, snapshot finalization is best-effort and represented as `mutation: None`, and a SQLite trigger fault-injection regression verifies the object write remains successful. | P1 | Fixed in this cycle |
| S3-007 | S3 | S3 object ETag and user metadata were persisted in separate property operations; replacing metadata removed old keys one at a time before setting new values, so storage failure could leave a partial property set. Put/Copy/multipart completion now replace ETag and metadata in one SQLite property transaction, source metadata reads happen before commit, and multipart metadata read errors fail before object commit instead of silently dropping metadata. A property transaction failure can still occur after the object version commit and remains a response/idempotency edge. | P1 | Partial-property corruption fixed; object/property cross-transaction atomicity open |
| QA-001 | S3 | Independent-client probe: boto3/botocore 1.34.46 passed 51/51 operations; rclone 1.60.1-DEV passed bucket listing, upload, list, download, 11 MiB multipart upload/download byte comparison, delete, Unicode/space/empty keys, checksum check, and upload/list/sync of 1005 objects across the 1000-key page boundary. | P1 | Verified for tested operations |
| QA-002 | WebDAV | cadaver 0.24 passed authenticated MKCOL, PUT, COPY, MOVE, PROPFIND listing, GET byte comparison, DELETE, and collection removal. System litmus passed all 104 basic/copymove/props/locks/http checks, including shared and depth-infinity locks; it reports warnings for a client-sent fragment and corrupt-token status expectation. | P1 | Full installed litmus suite verified; cadaver probe is automated in CI |
| QA-003 | RSYNC | System rsync 3.2.7 protocol 31 passed live against an isolated daemon: module discovery, recursive push/pull with empty directories and a 2 MiB file, checksum incremental update (32 literal bytes), `--delete --exclude` protection, and `--delete-excluded`. | P1 | Live client verified; automated real-client harness remains open |
| QA-004 | S3 test tooling | The installed botocore 1.34.46 S3 model predates conditional `DeleteObjects` fields and `PutObject`/`DeleteObject` conditions used by the probe. The probe now supplies those missing model members, including RFC 822 timestamp serialization; the complete 50/50 run passes. | P1 | Fixed in this cycle |

## Discovery and verification baseline

- Static analysis: `cargo clippy --workspace --all-targets -- -D warnings`.
- Formatting gate: `cargo fmt --all -- --check`.
- Regression baseline: `cargo test --workspace` plus protocol-specific end-to-end tests.
- Third-party tools found in the current development environment: `curl` 8.5.0,
  `rsync` 3.2.7 (protocol 31), `rclone` 1.60.1-DEV, `cadaver` 0.24,
  `boto3` 1.34.46, `botocore` 1.34.46, and `litmus`.
- Manual client checks in this environment are evidence for the operations
  listed above, not proof of complete protocol conformance. Automate these
  scenarios and add real rsync daemon client coverage to CI as follow-up work.

This is an environment snapshot, not a claim that the listed protocols are
fully conformant. Recheck tool availability and update this register when the
environment or test harness changes.
