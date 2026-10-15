/**
 * Polyfill for recent ECMAScript APIs used by pdf.js (pdfjs-dist 6) but still
 * missing on older browsers:
 *
 * 1. The "upsert" proposal methods `Map.prototype.getOrInsert` and
 *    `Map.prototype.getOrInsertComputed` (used everywhere by pdf.js on Map).
 * 2. The ES2025 binary-extension methods `Uint8Array.prototype.toHex`,
 *    `Uint8Array.fromHex`, `Uint8Array.prototype.toBase64` and
 *    `Uint8Array.fromBase64` (used for document fingerprints and digital
 *    signatures). pdf.js calls them as native methods, so the failure is a
 *    bare "a.toHex is not a function" thrown deep in the worker and rethrown
 *    on the main thread as UnknownErrorException.
 *
 * Everything is added only when the native method is missing, so browsers that
 * already ship these APIs (recent Chrome/Firefox/Safari) are unaffected.
 *
 * Import sites: main.tsx (entry chunk, covers the main thread) and
 * pdf.worker.entry.ts (covers the worker world).
 */

/* ---------- Map "upsert" methods ---------- */

interface MapUpsertMethods<K, V> extends Map<K, V> {
  getOrInsert?: (key: K, value: V) => V;
  getOrInsertComputed?: (key: K, compute: (key: K) => V) => V;
}

const mapProto = Map.prototype as unknown as MapUpsertMethods<unknown, unknown>;

if (typeof mapProto.getOrInsert !== "function") {
  mapProto.getOrInsert = function (key, value) {
    // Native semantics: an existing key is returned untouched; the default is
    // only computed (and set) when the key is missing.
    if (this.has(key)) return this.get(key);
    this.set(key, value);
    return value;
  };
}

if (typeof mapProto.getOrInsertComputed !== "function") {
  mapProto.getOrInsertComputed = function (key, compute) {
    if (this.has(key)) return this.get(key);
    const value = compute(key);
    this.set(key, value);
    return value;
  };
}

/* ---------- TypedArray hex/base64 (ES2025) ---------- */

declare global {
  interface Uint8Array {
    toHex?(this: Uint8Array): string;
    toBase64?(this: Uint8Array): string;
  }
  interface Uint8ArrayConstructor {
    fromHex?(hex: string): Uint8Array;
    fromBase64?(base64: string): Uint8Array;
  }
}

const HEX_ALPHABET_UPPER = "0123456789ABCDEF";

if (typeof Uint8Array.prototype.toHex !== "function") {
  Uint8Array.prototype.toHex = function (this: Uint8Array): string {
    // Plain loop rather than map+join: this runs per document fingerprint and
    // on arrays that can be large, so it must stay cheap.
    let out = "";
    for (let i = 0; i < this.length; i++) {
      const byte = this[i];
      out += HEX_ALPHABET_UPPER[byte >> 4] + HEX_ALPHABET_UPPER[byte & 0xf];
    }
    return out.toLowerCase();
  };
}

if (typeof Uint8Array.fromHex !== "function") {
  Uint8Array.fromHex = function fromHex(hex: string): Uint8Array {
    if (hex.length % 2 !== 0) {
      throw new SyntaxError("fromHex: string length must be a multiple of 2");
    }
    if (!/^[0-9a-fA-F]*$/.test(hex)) {
      throw new SyntaxError("fromHex: string contains non-hexadecimal characters");
    }
    const out = new Uint8Array(hex.length / 2);
    for (let i = 0; i < out.length; i++) {
      out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
    }
    return out;
  };
}

if (typeof Uint8Array.prototype.toBase64 !== "function") {
  const B64_CHUNK = 0x8000; // stay well under the String.fromCharCode apply limit
  Uint8Array.prototype.toBase64 = function (this: Uint8Array): string {
    let binary = "";
    for (let i = 0; i < this.length; i += B64_CHUNK) {
      binary += String.fromCharCode(...this.subarray(i, i + B64_CHUNK));
    }
    return btoa(binary);
  };
}

if (typeof Uint8Array.fromBase64 !== "function") {
  Uint8Array.fromBase64 = function fromBase64(base64: string): Uint8Array {
    // ASCII whitespace between characters is ignored per spec; reversed padding
    // and non-alphabet characters keep atob's native InvalidCharacterError,
    // which is functionally equivalent to the strict-mode rejection.
    const cleaned = base64.replace(/\s+/g, "");
    const binary = atob(cleaned);
    const out = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      out[i] = binary.charCodeAt(i);
    }
    return out;
  };
}

export {};
