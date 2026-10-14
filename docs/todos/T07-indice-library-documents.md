# T07 — Indice su `library_documents(document_id)`

- **Importanza**: Media
- **Effort**: S (< 1 giorno, migrazione)
- **Dipendenze**: nessuna (migrazione `0011_*`)

## Contesto

La PK di `library_documents` è `(library_id, document_id)`, ma il filtro di accesso
in `routes/documents/mod.rs` (`push_filters`) cerca nella direzione opposta:
`access_ld.document_id = d.id` **per ogni riga candidata**. Senza indice dedicato,
SQLite risolve con scan della tabella di join.

## Proposta

Nuova migrazione con `CREATE INDEX idx_library_documents_document ON
library_documents (document_id)`. Valutare lo stesso per eventuali altri join
asimmetrici emersi da `EXPLAIN QUERY PLAN` sulla query lista.

## File coinvolti

- `apps/server/migrations/0011_library_documents_index.sql` (nuovo)
- `apps/server/tests/schema.rs` (assert sull'indice, seguendo i test esistenti)

## Accettazione

- `EXPLAIN QUERY PLAN` della lista mostra lookup via indice su `library_documents`.
- Migrazione verificata su DB esistente (forward-only, come la 0010).
