# T11 — Rate limiting sugli endpoint auth

- **Importanza**: Alta
- **Effort**: M (1–3 giorni, inclusi test)
- **Dipendenze**: nessuna

## Contesto

`routes/auth.rs` e `/api/auth/*` non hanno alcun limite di frequenza: login,
callback OAuth e resolve proxy sono esposti a brute force / enumeration senza
freni (il timeout 120 s non c'entra nulla).

## Proposta

Limiter per IP sugli endpoint sensibili (login, OAuth start/callback, resolve):
es. `tower_governor` o un bucket in-memory semplice (l'installazione è single-node
con SQLite, niente stato distribuito da gestire). 429 con `Retry-After`, log del
superamento. Escludere `/api/health` e le sessioni già valide.

## File coinvolti

- `apps/server/Cargo.toml` (nuova dipendenza, se esterna)
- `apps/server/src/routes/auth.rs`, `apps/server/src/routes/mod.rs`
- `apps/server/tests/auth.rs` (test: oltre soglia → 429; sotto soglia → ok)

## Accettazione

- Raffica oltre soglia da un IP → 429 con `Retry-After`.
- Uso normale e sessioni valide mai limitati.
- Nessuno stato condiviso fragile al restart (il reset del bucket al riavvio è accettabile e va documentato).
