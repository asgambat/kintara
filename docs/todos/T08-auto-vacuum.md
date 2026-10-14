# T08 — `auto_vacuum` incrementale + manutenzione

- **Importanza**: Bassa
- **Effort**: S (< 1 giorno)
- **Dipendenze**: nessuna

## Contesto

Re-scan, refresh e cancellazioni lasciano pagine libere nel file SQLite, che cresce
senza mai compattarsi. Su installazioni longeve con churn (watcher attivo) lo
spreco è significativo.

## Proposta

- Nuovi DB: `PRAGMA auto_vacuum = INCREMENTAL` alla creazione (in `db.rs`, prima
  delle migrazioni).
- DB esistenti: documentare il passaggio manuale (richiede rewrite) oppure
  `PRAGMA incremental_vacuum(N)` periodico all'avvio/schedulato.
- Non applicare mai `VACUUM` pieno all'avvio: blocca tutto su DB grandi.

## File coinvolti

- `apps/server/src/db.rs`
- `README.md` (nota operativa per DB esistenti)

## Accettazione

- Nuovi DB creati con `auto_vacuum` incrementale (`PRAGMA auto_vacuum` = 2).
- Avvio e migrazioni esistenti invariati (nessun rewrite implicito).
