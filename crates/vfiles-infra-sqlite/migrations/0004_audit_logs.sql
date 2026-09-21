-- 审计日志：记录登录、上传、下载等重要操作，用于事后追溯。
--
-- 设计要点：
-- 1. 只追加（append-only）：通过触发器禁止 UPDATE / DELETE，保证「只读、不可删除、不可修改」
--    是在数据库层强制的，而不是只靠接口不提供删除入口；
-- 2. 冗余记录 username / ip / user_agent，即使后来用户被改名或删除，
--    历史记录仍然可读（不依赖外键级联删除，user_id 不加 FK）；
-- 3. device 为服务端解析出的粗粒度设备/浏览器描述，便于直接展示与筛选。
CREATE TABLE audit_logs (
    id TEXT PRIMARY KEY NOT NULL,
    created_at TEXT NOT NULL,
    -- 匿名（如登录失败）时为 NULL
    user_id TEXT,
    username TEXT NOT NULL DEFAULT '',
    -- 动作：login.success / login.failure / file.upload / file.download / ...
    action TEXT NOT NULL,
    result TEXT NOT NULL CHECK (result IN ('success', 'failure')),
    -- 操作对象（路径、分享码等）
    target TEXT,
    ip TEXT,
    user_agent TEXT,
    device TEXT,
    -- 失败原因或附加说明
    detail TEXT
);

CREATE INDEX IF NOT EXISTS idx_audit_logs_created ON audit_logs(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_logs_user ON audit_logs(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_logs_action ON audit_logs(action, created_at DESC);

-- 只读保证：任何修改/删除都会被数据库拒绝
CREATE TRIGGER IF NOT EXISTS audit_logs_block_update
BEFORE UPDATE ON audit_logs
BEGIN
    SELECT RAISE(ABORT, 'audit logs are append-only and cannot be modified');
END;

CREATE TRIGGER IF NOT EXISTS audit_logs_block_delete
BEFORE DELETE ON audit_logs
BEGIN
    SELECT RAISE(ABORT, 'audit logs are append-only and cannot be deleted');
END;
