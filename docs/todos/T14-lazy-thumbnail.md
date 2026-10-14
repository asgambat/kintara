# T14 — Verificare `loading="lazy"` sulle thumbnail

- **Importanza**: Media
- **Effort**: S (verifica + eventuale one-liner)
- **Dipendenze**: nessuna
- **Stato**: ✅ implementato (verifica + `decoding="async"`)

## Contesto

Verifica completata: la griglia (`DocumentCard.tsx`) aveva già `loading="lazy"` e il
contenitore riserva lo spazio (`aspect-ratio: 3/4` in `App.css`, niente layout shift).
Mancava solo `decoding="async"`, mai usato nel progetto: la decodifica avveniva sul
main thread.

## Proposta

Aggiunto `decoding="async"` a `DocumentCard.tsx` (griglia), `DetailsSidebar.tsx`
(cover) e `ImportModal.tsx` (anteprima). Esclusi: loghi (above-fold, già
sized), preview AI (data-URL, nessun fetch).

## File coinvolti

- `apps/web/src/components/` (griglia documenti, da individuare via search `thumbnail`)

## Accettazione

- `npm run test --workspace apps/web`: 50/50 verdi; `tsc --noEmit`: exit 0.
