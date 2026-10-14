# T05 — Thumbnail in background con placeholder

- **Importanza**: Media
- **Effort**: M (1–3 giorni)
- **Dipendenze**: T04 (upload streaming) consigliato prima

## Contesto

`media::generate_thumbnail` (via `pdftoppm`) gira **dentro** la richiesta di upload
e dentro lo scan: un PDF con pagina iniziale pesante rallenta la risposta HTTP.

## Proposta

Rispondere subito con entry senza thumbnail e accodare la generazione a un worker
in background (canale `mpsc` + task dedicato); il frontend mostra un placeholder e
riceve la thumbnail al refresh successivo (il nome contiene già id+timestamp, quindi
la cache resta corretta). Retry limitato su fallimento, log e stop.

## File coinvolti

- `apps/server/src/media.rs`
- `apps/server/src/routes/documents/upload.rs`
- `apps/server/src/scanner/index.rs`
- `apps/server/src/state.rs` (coda worker)
- `apps/web/src/components/*` (placeholder, se assente)

## Accettazione

- Latenza p99 dell'upload indipendente dalla pesantezza del PDF.
- La thumbnail appare entro N secondi senza intervento utente.
- Nessun file orfano in `thumbnail_dir` a worker fallito.
