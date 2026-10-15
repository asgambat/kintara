/**
 * Polyfill for the ECMAScript "upsert" proposal methods `Map.prototype.getOrInsert`
 * and `Map.prototype.getOrInsertComputed`.
 *
 * pdf.js (pdfjs-dist 5.3+ and everywhere in the 6.x line) calls these as native
 * Map methods, but engines shipped them only recently: Safari/WebKit and older
 * Chrome/Firefox releases still lack them, and pdf.js then throws
 * "getOrInsertComputed is not a function" on the first PDF open, in both the
 * main thread and the worker. This module adds the missing methods to
 * Map.prototype exactly as spec'd, so the same code runs everywhere; browsers
 * that already have the native methods are untouched (the guard is a no-op).
 *
 * Import sites: main.tsx (entry chunk, covers the main thread) and
 * pdf.worker.entry.ts (covers the worker world).
 */

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

export {};
