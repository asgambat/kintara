# T12 — `React.lazy` per readers e pannello AI

- **Importanza**: Media
- **Effort**: S (< 1 giorno)
- **Dipendenze**: nessuna
- **Stato**: ✅ implementato (`ReaderPanes-*.js` 51 kB + `AiPanel-*.js` 11 kB separati in dist)

## Contesto

`react-markdown` (+ `remark-gfm`) è importata **staticamente** da
`MarkdownReader.tsx` e `AiPanel.tsx`: finisce nel chunk principale pagata da tutti,
anche da chi apre solo PDF. Stesso trucco già applicato con successo a Shiki
(import dinamico) — da replicare.

## Proposta

`React.lazy` + `Suspense` applicati in `App.tsx` con fallback `role="status"`.
Verificato: `dist/index.html` precarica solo `vendor` (statico); i chunk lazy e il
precache SW (`/`, `/index.html`, manifest, logo) non li anticipano.

## File coinvolti

- `apps/web/src/components/ReaderPanes.tsx`
- `apps/web/src/components/AiPanel.tsx` e chi lo monta
- `apps/web/vite.config.ts` (verifica chunk, nessun cambio atteso)
- `apps/web/public/sw.js` (verifica precache — vedi nota sopra)

## Accettazione

- Build verde, test web 52/52 verdi, `tsc --noEmit` exit 0.
