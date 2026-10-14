# T06 — Timeout + kill sui processi poppler

- **Importanza**: Alta
- **Effort**: S (< 1 giorno)
- **Dipendenze**: nessuna

## Contesto

`pdftotext`, `pdfinfo` e `pdftoppm` sono lanciati senza timeout
(`text_extraction.rs`, `media.rs`): un PDF corrotto o un poppler appeso blocca
**per sempre** lo scan o la richiesta che l'ha invocato.

## Proposta

Avvolgere ogni `Command` in `tokio::time::timeout` (es. 60–120 s, env
`KINTARA_POPPLER_TIMEOUT_S`) con `kill_on_drop(true)`. Scaduto il timeout, il
documento degrada a `text_status = 'failed'` / thumbnail assente — mai fatale,
come già previsto per gli altri errori poppler.

## File coinvolti

- `apps/server/src/text_extraction.rs` (`extract_pdf_with`)
- `apps/server/src/media.rs` (`extract_pdf_metadata`, `generate_thumbnail`)
- `apps/server/src/config.rs` (nuova env)

## Accettazione

- PDF che appende `pdftotext` produce `failed` entro il timeout, senza bloccare scan.
- Test con binary finto che dorme oltre il timeout (come il test esistente sul binary mancante).
- Nessun processo zombie (`kill_on_drop` verificato).
