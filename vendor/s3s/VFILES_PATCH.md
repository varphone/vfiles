# Local s3s patch

This directory vendors s3s 0.16.1 (Apache-2.0) with the upstream library and
unit tests retained. VFiles adds `S3ServiceBuilder::set_path_prefix`; it removes
the configured prefix only from S3 operation path classification. Signature
verification continues to receive the original request URI, so AWS SigV4
canonical URI validation remains unchanged.

The upstream integration-test targets are not registered because their shared
fixture corpus is not part of the published crate source; the source files are
retained for reference. To refresh this copy, copy the upstream crate source,
reapply the `path_prefix` changes in `service.rs`, `http/request.rs`,
`http/de.rs`, and `ops/mod.rs`, and run the workspace s3s unit tests plus the
VFiles S3 SigV4 probe with both root and prefixed endpoints.
