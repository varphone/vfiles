import server from '../src/index';

describe('auth me endpoint', () => {
  it('returns enabled=false when auth disabled', async () => {
    const res = await server.fetch(new Request('http://localhost/api/auth/me'));
    expect(res.status).toBe(200);
    const data = await res.json();
    expect(data.success).toBe(true);
    expect(data.data).toHaveProperty('enabled', false);
  });
});
