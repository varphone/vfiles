-- Keep the internal lookup key separate from the short code shown to users.
ALTER TABLE shares ADD COLUMN public_code TEXT;
CREATE UNIQUE INDEX idx_shares_public_code ON shares(public_code);

-- Preserve existing eight-character links when adding the public-code column.
UPDATE shares SET public_code = code WHERE length(code) = 8;
