# T09 — Snapshot schedulati del DB (oltre il pre-migrazione)

- **Importanza**: Media
- **Effort**: S (< 1 giorno)
- **Dipendenze**: nessuna (riusa la rotazione a 5 già in `db.rs`)

## Contesto

Il backup esiste solo **prima delle migrazioni** (`backup_before_migrate` in
`db.rs`). Se il DB si corrompe a runtime (blackout su SQLite, disco pieno), l'unica
copia può essere vecchia di mesi.

## Proposta

Snapshot schedulato (es. giornaliero via env `KINTARA_BACKUP_INTERVAL_HOURS`, 0 =
disabilitato), riusando `backups/` e la rotazione esistente. Usare l'API di backup
online di SQLite (o copia del file con checkpoint WAL) per non bloccare i reader;
mai copiare il file a freddo mentre è scritto.

## File coinvolti

- `apps/server/src/db.rs` (estrarre `prune_backups`, aggiungere scheduler)
- `apps/server/src/main.rs` (avvio task schedulato)
- `apps/server/src/config.rs` (nuova env)
- `README.md` + `docker-compose.yml` (documentare volume `backups/`)

## Accettazione

- Con interval impostato, un nuovo snapshot appare in `backups/` senza restart.
- Rotazione rispettata (max 5 di default).
- Ripristino documentato: stop → swap file → start.
