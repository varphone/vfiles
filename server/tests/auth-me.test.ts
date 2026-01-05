/// @vitest-environment node
import { Hono } from 'hono';
import { createAuthRoutes } from '../src/routes/auth.routes.js';

describe('auth me endpoint', () => {
  it('returns enabled=false when auth disabled', async () => {
    const cfg = { enabled: false, secret: '', cookieName: 'vfiles_session', tokenTtlSeconds: 60, allowRegister: true, loginRateLimit: { enabled: false, windowMs: 60000, max: 10 }, passwordResetTokenTtlSeconds: 1800, emailLoginCodeTtlSeconds: 600 };
    const app = new Hono();
    app.route('/api/auth', createAuthRoutes(cfg as any, { getUserById: async () => undefined } as any, { isEnabled: () => false } as any, {}));

    const res = await app.fetch(new Request('http://localhost/api/auth/me'));
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(data.success).toBe(true);
    expect(data.data).toHaveProperty('enabled', false);
  });
});
