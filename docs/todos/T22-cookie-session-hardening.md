# T22 — Hardening del cookie di sessione

- **Importanza**: Alta
- **Effort**: S (< 1 giorno)

## Contesto

Il cookie `kintara_session` è la singola porta d'ingresso dell'applicazione (owner,
API key delle sessioni AI, cancellazioni). Il suo attributo set è costruito in
`auth.rs` (funzione di creazione sessione): conviene verificare esplicitamente che
porti `HttpOnly`, `SameSite` dichiarato e — discriminato dallo schema di
`KINTARA_PUBLIC_URL` (https) — `Secure`. Un cookie senza `SameSite` è inviabile da
pagine esterne (CSRF a fondo su endpoint POST come mutazioni e settings) e uno senza
`HttpOnly` è rubabile da uno script che trapassa il DOM.

## Proposta

1. Revisione dell'header `Set-Cookie` in `auth.rs`, con la stessa scelta già
   fatta per OAuth: `Secure` SOLO quando `KINTARA_PUBLIC_URL` (o il gruppo usato
   a scheme) è https, `HttpOnly: true`, `SameSite=Lax` (la PWA di Kintara non usa
   redirect cross-site transazionali), `Path=/api` (o `/` se il receiver ne
   richiede il fallback), Max-Age coerente con la TTL delle sessioni in DB.
2. Test: risposta del login/route refresh include gli attributi attesi (http e
   https rispettivamente).
3. Se il cookie è già corretto: chiusura con nota semplici leggibili (significa
   che la "vulnerabilità" era negativa).

## File coinvolti

- `apps/server/src/auth.rs` (`create_session` + già lettura del cookie nelle routes)
- `apps/server/tests/auth.rs` (assert sugli attributi del Set-Cookie)

## Accettazione

- `Set-Cookie` porta `HttpOnly`, `SameSite=Lax` e, con public URL https, `Secure`.
- Test automatici legano gli attributi alla risposta reale (http e https simulata).
- Il flusso OAuth a fine callback non è più rotto in conseguenza degli oratori.
