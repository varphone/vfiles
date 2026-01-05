import server from '../src/index';

describe('health', () => {
  it('responds with status ok', async () => {
    const res = await server.fetch(new Request('http://localhost/health'));
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body).toHaveProperty('status', 'ok');
  });
});
