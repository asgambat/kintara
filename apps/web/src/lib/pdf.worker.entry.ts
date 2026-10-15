/**
 * Wrapper entry for the pdf.js worker.
 *
 * The worker runs in a separate JS world, so the main-thread polyfill import in
 * main.tsx does not reach it — yet pdf.js uses the modern Map upsert methods and
 * the Uint8Array hex/base64 APIs there too. Importing the polyfill before the
 * worker bundle means they exist before pdf.js code runs, on browsers that lack
 * them natively.
 *
 * pdf.js instantiates the worker itself from `GlobalWorkerOptions.workerSrc`
 * with `{ type: "module" }`, and the bundle is built by Vite as an ES module
 * worker (`worker.format: "es"` in vite.config.ts) because pdf.worker.min.mjs
 * reads `import.meta.url`.
 */
import "./modern-api-polyfill";
import "pdfjs-dist/build/pdf.worker.min.mjs";
