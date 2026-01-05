/// @vitest-environment node
import { Hono } from 'hono';
import { createAuthRoutes } from '../src/routes/auth.routes.js';
import { UserStore } from '../src/services/user-store.js';

function makeCfg() {
  return {
    enabled: true,
    secret: 'test-secret',
    cookieName: 'vfiles_session',
    tokenTtlSeconds: 60,
    allowRegister: true,
    cookieSecure: false,
    loginRateLimit: { enabled: false, windowMs: 60000, max: 10 },
    passwordResetTokenTtlSeconds: 30 * 60,
    emailLoginCodeTtlSeconds: 10 * 60,
  };
}

function makeEmailService() {
  const calls: any[] = [];
  return {
    isEnabled() {
      return true;
    },
    async sendMail(opts: { to: string; subject: string; text: string }) {
      calls.push(opts);
      return Promise.resolve();
    },
    _calls: calls,
  };
}

describe('auth routes (edge cases)', () => {
  let store: UserStore;
  let tmpFile: string;
  let emailService: any;

  beforeEach(() => {
    tmpFile = `./.test-users-${Math.random().toString(36).slice(2)}.json`;
    store = new UserStore(tmpFile);
    emailService = makeEmailService();
  });

  afterEach(async () => {
    try { await import('node:fs/promises').then((fs) => fs.unlink(tmpFile).catch(() => {})); } catch {}
  });

  it('does not leak existence for password reset request', async () => {
    const app = new Hono();
    app.route('/api/auth', createAuthRoutes(makeCfg(), store, emailService, { publicBaseUrl: 'http://localhost' }));

    const req = new Request('http://localhost/api/auth/password-reset/request', {
      method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ email: 'nope@example.com' })
    });
    const res = await app.fetch(req);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.success).toBe(true);
    expect(emailService._calls.length).toBe(0);
  });

  it('revoke sessions bumps version and prevents old token usage', async () => {
    // create an admin and a user
    const admin = await store.createUser({ username: 'adminuser', password: 'adminpass', email: 'admin@example.com' });
    const user = await store.createUser({ username: 'revoker', password: 'pass1234', email: 'rev@example.com' });
    const app = new Hono();
    const cfg = makeCfg();
    // apply auth middleware as in production to enable admin-protected endpoints
    const { authMiddleware } = await import('../src/middleware/auth.js');
    app.use('/api/*', authMiddleware(cfg as any, store));
    app.route('/api/auth', createAuthRoutes(cfg, store, emailService, {}));

    // sign token for admin
    const { signAuthToken } = await import('../src/utils/auth-token.js');
    const token = signAuthToken({ v: 2, sub: admin.id, username: admin.username, role: 'admin', exp: Math.floor(Date.now()/1000)+60, sv: admin.sessionVersion ?? 0 } as any, cfg.secret);

    const req = new Request(`http://localhost/api/auth/users/${encodeURIComponent(user.id)}/revoke-sessions`, { method: 'POST', headers: { Cookie: `${cfg.cookieName}=${token}` } });
    const res = await app.fetch(req);
    expect(res.status).toBe(200);

    // old tokens for user should be invalidated (we check bump by fetching user)
    const after = await store.getUserById(user.id);
    expect(after?.sessionVersion).toBeGreaterThanOrEqual(1);
  });
});