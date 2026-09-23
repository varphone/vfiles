# Protocol compatibility audit

Use this register to turn protocol review into a repeatable loop: discover a
concrete issue, record its evidence and priority, fix one item, run the relevant
checks, and record the commit that closes it.

## Findings

| ID | Protocol | Finding and evidence | Priority | State |
| --- | --- | --- | --- | --- |
| DAV-001 | WebDAV | `Overwrite` was parsed case-sensitively. RFC 4918 defines `T`/`F` as ABNF string literals, which are case-insensitive; a compliant lowercase `f` was rejected. This cycle adds parser and request-level regression coverage. | P1 | Fixed in this cycle |
| QA-001 | S3 | No installed AWS SDK/CLI client is available (`boto3`, `aws`, `s3cmd`, and `rclone` are absent), so the S3 API tests do not yet have independent third-party client evidence. | P1 | Open |
| QA-002 | WebDAV | No dedicated DAV client or compliance suite (`cadaver`, `litmus`) is installed. Current protocol coverage relies on the in-process end-to-end test and RFC checks. | P1 | Open |
| QA-003 | RSYNC | The system rsync client is installed, but the repository has no automated test that runs it against the daemon; existing coverage is protocol fixtures and in-process tests. | P1 | Open |

## Discovery and verification baseline

- Static analysis: `cargo clippy --workspace --all-targets -- -D warnings`.
- Formatting gate: `cargo fmt --all -- --check`.
- Regression baseline: `cargo test --workspace` plus protocol-specific end-to-end tests.
- Third-party tools found in the current development environment: `curl` 8.5.0
  and `rsync` 3.2.7. `cadaver`, `litmus`, `rclone`, `aws`, and `s3cmd` were not
  found; importing Python `boto3` fails because it is not installed.

This is an environment snapshot, not a claim that the listed protocols are
fully conformant. Recheck tool availability and update this register when the
environment or test harness changes.
