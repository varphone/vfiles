/// @vitest-environment node
import { Hono } from 'hono';

describe('health', () => {
  it('responds with status ok', async () => {
    const app = new Hono();
    app.get('/health', (c) => c.json({ status: 'ok', timestamp: new Date().toISOString() }));
    const res = await app.fetch(new Request('http://localhost/health'));
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body).toHaveProperty('status', 'ok');
  });
});
