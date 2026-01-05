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

describe('auth routes', () => {
  let store: UserStore;
  let tmpFile: string;
  let emailService: any;

  beforeEach(() => {
    tmpFile = `./.test-users-${Math.random().toString(36).slice(2)}.json`;
    store = new UserStore(tmpFile);
    emailService = makeEmailService();
  });

  afterEach(async () => {
    try {
      await Promise.resolve();
      await import('node:fs/promises').then((fs) => fs.unlink(tmpFile).catch(() => {}));
    } catch {}
  });

  it('registers a user', async () => {
    const app = new Hono();
    app.route('/api/auth', createAuthRoutes(makeCfg(), store, emailService, { publicBaseUrl: '' }));

    const req = new Request('http://localhost/api/auth/register', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ username: 'reguser', password: 'passw0rd' }),
    });
    const res = await app.fetch(req);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.success).toBe(true);
    expect(body.data.user.username).toBe('reguser');
  });

  it('login fails with wrong password', async () => {
    await store.createUser({ username: 'userx', password: 'correct' });
    const app = new Hono();
    app.route('/api/auth', createAuthRoutes(makeCfg(), store, emailService, {}));

    const req = new Request('http://localhost/api/auth/login', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ username: 'userx', password: 'wrongpw' }),
    });
    const res = await app.fetch(req);
    const txt = await res.text();
    const body = JSON.parse(txt);
    expect(res.status).toBe(401);
    expect(body.success).toBe(false);
  });

  it('password reset request sends email for existing account', async () => {
    await store.createUser({ username: 'usere', password: 'pass1234', email: 'e@example.com' });
    const app = new Hono();
    app.route('/api/auth', createAuthRoutes(makeCfg(), store, emailService, { publicBaseUrl: 'http://localhost' }));

    const req = new Request('http://localhost/api/auth/password-reset/request', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ email: 'e@example.com' }),
    });
    const res = await app.fetch(req);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.success).toBe(true);
    expect(emailService._calls.length).toBeGreaterThan(0);
  });

  it('email login request sends email for existing account', async () => {
    await store.createUser({ username: 'userf', password: 'pass1234', email: 'f@example.com' });
    const app = new Hono();
    app.route('/api/auth', createAuthRoutes(makeCfg(), store, emailService, {}));

    const req = new Request('http://localhost/api/auth/email-login/request', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ email: 'f@example.com' }),
    });
    const res = await app.fetch(req);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.success).toBe(true);
    expect(emailService._calls.length).toBeGreaterThan(0);
  });
});
