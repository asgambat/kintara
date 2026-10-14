# T17 — `Cache-Control: immutable` su `/assets/*`

- **Importanza**: Bassa
- **Effort**: S (< 1 giorno)
- **Dipendenze**: nessuna
- **Stato**: ✅ implementato (middleware dedicato, nessuna nuova dipendenza)

## Contesto

Vite emette bundle con hash nel nome: sono immutabili per costruzione, ma il server
(`ServeDir` in `routes/mod.rs`) non invia `Cache-Control: immutable`, quindi il
browser rivalida. Oggi la cache è affidata al service worker — chi lo disabilita o
al primo caricamento paga ricaricamenti inutili.

## Proposta

Implementato come middleware `immutable_assets` in `routes/mod.rs` (niente feature
`set-header`: solo path `/assets/*` con hash stile Vite `nome-<8+alnum>.ext`, solo
risposte 2xx. `assets/brand/*` esclusi dal pattern: stesso nome, byte mutevoli.

## File coinvolti

- `apps/server/src/routes/mod.rs` (middleware + `is_hashed_asset`)
- `apps/server/tests/api.rs` (nuovo test header)

## Accettazione

- `GET /assets/index-*.js` → `cache-control` con `immutable`.
- `GET /index.html` e `/api/*` invariati (nessun `immutable`).
- Test `hashed_assets_are_immutable_but_brand_aliases_stay_revalidatable` verde
  (da confermare in CI: niente toolchain Rust qui).
