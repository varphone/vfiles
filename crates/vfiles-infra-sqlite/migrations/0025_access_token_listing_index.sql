DROP INDEX access_tokens_user_idx;
CREATE INDEX access_tokens_user_listing_idx
ON access_tokens(user_id, created_at DESC, id DESC);
