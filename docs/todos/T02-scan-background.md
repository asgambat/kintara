# T02 — Scan di avvio in background (non blocca il serve)

- **Importanza**: Alta
- **Effort**: S (< 1 giorno)
- **Dipendenze**: nessuna (sblocca T16: rende affidabile l'healthcheck)
- **Stato**: ✅ implementato (`tokio::spawn` dello scan in `main`)

## Contesto

In `apps/server/src/main.rs` lo scan (`scanner::full_scan(&state).await`) gira
**prima** di `bind`/`serve`: il server non risponde finché lo scan non finisce.
Con librerie grandi l'healthcheck Docker (`start-period: 20s`) fallisce e il
container risulta unhealthy pur funzionando.

## Proposta

Lanciare lo scan in un `tokio::spawn` dopo il bind, con log di inizio/fine. Il
flag `scan_on_start` resta invariato; la libreria è leggibile da subito dai dati
già indicizzati (comportamento già previsto per scan falliti).

## File coinvolti

- `apps/server/src/main.rs`
- `docker/Dockerfile` (eventuale ritocco `start-period`, vedi T16)

## Accettazione

- Il socket risponde a `/api/health` entro pochi secondi anche con scan in corso.
- Lo scan completa e logga `library scan complete` come oggi.
- Shutdown graceful durante lo scan non corrompe il DB (transazioni già atomiche).
