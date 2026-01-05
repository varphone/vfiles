/// @vitest-environment node
import { validateRequiredString } from '../src/utils/validation.js';

describe('validation utils', () => {
  it('validates presence and length', () => {
    const ok = validateRequiredString('abc', 'field', { minLength: 2 });
    expect(ok.ok).toBe(true);
  });

  it('rejects short string', () => {
    const res = validateRequiredString('a', 'f', { minLength: 2 });
    expect(res.ok).toBe(false);
    expect(res.status).toBe(400);
  });
});