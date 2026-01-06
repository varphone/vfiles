/// @vitest-environment node
import { signAuthToken, verifyAuthToken } from "../src/utils/auth-token.js";

describe("auth-token utils", () => {
  it("signs and verifies token v2 with sessionVersion", () => {
    const secret = "s3cr3t";
    const payload = {
      v: 2,
      sub: "u1",
      username: "u1",
      role: "user",
      exp: Math.floor(Date.now() / 1000) + 60,
      sv: 3,
    };
    const token = signAuthToken(payload as any, secret);
    const res = verifyAuthToken(token, secret);
    expect(res.ok).toBe(true);
    expect(res.payload.sub).toBe("u1");
    expect(res.payload.sv).toBe(3);
  });

  it("rejects token with wrong secret", () => {
    const secret = "s3cr3t";
    const token = signAuthToken(
      {
        v: 2,
        sub: "u1",
        username: "u1",
        role: "user",
        exp: Math.floor(Date.now() / 1000) + 60,
        sv: 0,
      } as any,
      secret,
    );
    const res = verifyAuthToken(token, "wrong");
    expect(res.ok).toBe(false);
  });
});
