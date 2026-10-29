import { describe, expect, it } from "vitest";

import { sha256Bytes, sha256Digest, toHex } from "./sha256";

const enc = (s: string) => new TextEncoder().encode(s);

describe("sha256 fallback for plain-HTTP origins", () => {
  it("matches the standard test vectors", () => {
    expect(toHex(sha256Bytes(enc("")))).toBe(
      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
    expect(toHex(sha256Bytes(enc("abc")))).toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
    expect(
      toHex(sha256Bytes(enc("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"))),
    ).toBe("248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
  });

  it("agrees with Web Crypto on multi-block and non-ASCII input", async () => {
    for (const text of ["a".repeat(1000), "₹ UPI पेमेंट 🙂", "x".repeat(55), "y".repeat(64)]) {
      const expected = new Uint8Array(await crypto.subtle.digest("SHA-256", enc(text)));
      expect(toHex(sha256Bytes(enc(text)))).toBe(toHex(expected));
      expect(toHex(await sha256Digest(enc(text)))).toBe(toHex(expected));
    }
  });
});
