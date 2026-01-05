import crypto from "node:crypto";

export interface AuthTokenPayload {
  v: 1;
  sub: string; // user id
  username: string;
  role: string;
  exp: number; // seconds
}

function base64UrlEncode(buf: Uint8Array): string {
  return Buffer.from(buf)
    .toString("base64")
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replaceAll("=", "");
}

function base64UrlDecode(s: string): Uint8Array {
  const pad = s.length % 4 === 0 ? "" : "=".repeat(4 - (s.length % 4));
  const b64 = s.replaceAll("-", "+").replaceAll("_", "/") + pad;
  return new Uint8Array(Buffer.from(b64, "base64"));
}

function timingSafeEqual(a: string, b: string): boolean {
  const aBuf = Buffer.from(a);
  const bBuf = Buffer.from(b);
  if (aBuf.length !== bBuf.length) return false;
  return crypto.timingSafeEqual(aBuf, bBuf);
}

export function signAuthToken(
  payload: AuthTokenPayload,
  secret: string,
): string {
  const payloadJson = JSON.stringify(payload);
  const payloadB64 = base64UrlEncode(Buffer.from(payloadJson, "utf-8"));
  const sig = crypto.createHmac("sha256", secret).update(payloadB64).digest();
  const sigB64 = base64UrlEncode(sig);
  return `${payloadB64}.${sigB64}`;
}

export function verifyAuthToken(
  token: string,
  secret: string,
): { ok: true; payload: AuthTokenPayload } | { ok: false; reason: string } {
  const parts = (token || "").split(".");
  if (parts.length !== 2) return { ok: false, reason: "bad_format" };
  const [payloadB64, sigB64] = parts;
  if (!payloadB64 || !sigB64) return { ok: false, reason: "bad_format" };

  const expected = base64UrlEncode(
    crypto.createHmac("sha256", secret).update(payloadB64).digest(),
  );
  if (!timingSafeEqual(expected, sigB64)) {
    return { ok: false, reason: "bad_sig" };
  }

  let payload: AuthTokenPayload;
  try {
    payload = JSON.parse(
      Buffer.from(base64UrlDecode(payloadB64)).toString("utf-8"),
    );
  } catch {
    return { ok: false, reason: "bad_payload" };
  }

  if (!payload || payload.v !== 1) return { ok: false, reason: "bad_payload" };
  if (typeof payload.exp !== "number")
    return { ok: false, reason: "bad_payload" };
  const nowSec = Math.floor(Date.now() / 1000);
  if (payload.exp <= nowSec) return { ok: false, reason: "expired" };

  return { ok: true, payload };
}
