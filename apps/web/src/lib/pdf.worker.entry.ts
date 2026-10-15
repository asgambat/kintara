/**
 * Wrapper entry for the pdf.js worker.
 *
 * The worker runs in a separate JS world, so the main-thread polyfill import in
 * pdf.ts does not reach it — yet pdf.js uses the `Map` upsert methods there too
 * (15 call sites in pdfjs-dist 6). Importing the polyfill before the worker
 * bundle means the methods exist before pdf.js code runs, on browsers that lack
 * them natively.
 *
 * pdf.js instantiates the worker itself from `GlobalWorkerOptions.workerSrc`
 * with `{ type: "module" }`, and the bundle is built by Vite as an ES module
 * worker (`worker.format: "es"` in vite.config.ts) because pdf.worker.min.mjs
 * reads `import.meta.url`.
 */
import "./map-upsert-polyfill";
import "pdfjs-dist/build/pdf.worker.min.mjs";
