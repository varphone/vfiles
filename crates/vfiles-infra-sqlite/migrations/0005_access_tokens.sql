-- 访问令牌：给 CLI / 构建系统等程序使用的 API 凭证。
-- 只保存 SHA-256 摘要，明文仅在创建时返回一次。
CREATE TABLE access_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    token_prefix TEXT NOT NULL,
    scopes TEXT NOT NULL DEFAULT 'full',
    expires_at TEXT,
    last_used_at TEXT,
    revoked_at TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX access_tokens_user_idx ON access_tokens(user_id);

-- 按摘要查找是鉴权热路径
CREATE INDEX access_tokens_hash_idx ON access_tokens(token_hash);
