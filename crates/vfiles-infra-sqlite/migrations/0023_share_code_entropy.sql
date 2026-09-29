-- Previous versions truncated UUIDv4 share codes to eight hex characters.
-- Rotate those 32-bit bearer credentials to 128-bit codes during upgrade.
UPDATE shares
SET code = lower(hex(randomblob(16)))
WHERE length(code) < 32;
