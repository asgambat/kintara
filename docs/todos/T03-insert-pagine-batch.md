# T03 — INSERT pagine in batch in `extract_and_store`

- **Importanza**: Media
- **Effort**: S (< 1 giorno)
- **Dipendenze**: nessuna

## Contesto

`text_extraction::extract_and_store` (`apps/server/src/text_extraction.rs`) esegue
**un INSERT per pagina** in `document_pages`: un libro da 500 pagine = 500
round-trip (dentro una transazione, ma comunque 500 statement preparati/eseguiti).

## Proposta

Costruire un unico INSERT multi-riga (a blocchi di es. 200 pagine) con
`sqlx::QueryBuilder`. Stesso contenuto, stessa transazione, molti meno statement.

## File coinvolti

- `apps/server/src/text_extraction.rs` (`extract_and_store`)
- test esistenti su `document_pages` (devono restare verdi senza modifiche)

## Accettazione

- Il contenuto di `document_pages` è identico prima/dopo (test di confronto).
- Tempo di `extract_and_store` su fixture da 500 pagine ridotto sensibilmente.
- Nessun superamento dei limiti SQLite sulle variabili per statement (chunking).
