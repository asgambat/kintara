import * as pdfjsLib from "pdfjs-dist";
// The modern-API polyfill (Map upsert, Uint8Array hex/base64 — see
// lib/modern-api-polyfill.ts) is imported once in main.tsx, the entry chunk, so
// it always executes before any PDF code on browsers without the native APIs.
// Build the worker from our own wrapper so the same polyfill runs inside
// the worker world too, before pdf.js code. Vite bundles it as an ES module
// worker (`worker.format: "es"`) and `?worker&url` yields the hashed URL.
// The desktop version pulled the worker from unpkg, which broke any
// offline use — and a NAS is frequently offline or firewalled.
import workerUrl from "./pdf.worker.entry.ts?worker&url";

pdfjsLib.GlobalWorkerOptions.workerSrc = workerUrl;

/**
 * Runtime resources pdf.js fetches on demand, served from `/pdfjs/` by the
 * plugin in vite.config.ts.
 *
 * Without `wasmUrl`, pdf.js cannot decode JPEG 2000 or JBIG2 images — the two
 * codecs scanners and magazine PDFs reach for most. The page still renders its
 * text and vector art, so the failure looks like "the pictures are missing"
 * rather than an error, and nothing is logged loudly enough to notice.
 *
 * `iccUrl` covers ICC colour profiles, `cMapUrl` CJK text, and
 * `standardFontDataUrl` the base-14 fonts a PDF is allowed to assume exist.
 */
export const pdfAssetOptions = {
  cMapUrl: "/pdfjs/cmaps/",
  cMapPacked: true,
  standardFontDataUrl: "/pdfjs/standard_fonts/",
  wasmUrl: "/pdfjs/wasm/",
  iccUrl: "/pdfjs/iccs/",
} as const;

export { pdfjsLib };
