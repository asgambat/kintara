# T13 — Virtualizzazione della lista documenti

- **Importanza**: Alta
- **Effort**: M (1–3 giorni, inclusi test)
- **Dipendenze**: nessuna
- **Stato**: ✅ implementato (`@tanstack/react-virtual` 3.14, lane misurate, fallback non virtualizzato ≤60)

## Contesto

Il "Load more" accumula pagine preservando i nodi DOM (da CHANGELOG 1.13.8): oltre
qualche migliaio di documenti lo scroll e i re-render diventano pesanti,
soprattutto su telefoni. Il server pagina già correttamente (limite 200, default
50) — il collo è solo lato DOM.

## Proposta

Virtualizzate le righe con `@tanstack/react-virtual` (`lanes` da `ResizeObserver` +
misura dinamica, overscan 4). Ogni riga riusa `.document-grid`, quindi le card
restano identiche; sotto i 60 documenti il markup originale è invariato. "Load more",
conteggio, sort e Recent invariati.

## File coinvolti

- `apps/web/src/components/DocumentGrid.tsx`, `apps/web/src/lib/grid-lanes.ts` (nuovo)
- `apps/web/test/grid-lanes.test.ts` (nuovo): aritmetica lane + degrado a 1

## Accettazione

- Test web 52/52 verdi, build + `tsc` verdi. Da verificare a mano su libreria
grande: nodi DOM limitati a viewport + overscan, nessuno shift dello scroll al
cambio lane (resize/drawer).
