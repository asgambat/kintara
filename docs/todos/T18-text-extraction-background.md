# T18 — Estrazione testo fuori dalla richiesta di upload

- **Importanza**: Media
- **Effort**: M (1–2 giorni)
- **Dipendenze**: nessuna (riusa, per analogia, la coda del worker introdotta con T05)

## Contesto

Come la thumbnail prima di T05, `extract_and_store` gira ancora **dentro** la richiesta
di upload (`routes/documents/upload.rs`): `pdftotext` su un PDF denso (testo lungo,
pagina dopo pagina) tiene la risposta HTTP aperta per secondi — proprio quando la
generazione della cover(TimeSpan) è stata spostata fuori.

La latenza accettata da T05 ("indipendente dalla pesantezza del PDF") resta in realtà
dipendente dal contenuto del PDF finché il testo si estrae in linea.

## Proposta

Accodare `extract_and_store` al medesimo pattern già funzionante per le thumbnail:

- una segunda coda (canale mpsc unbounded) dedicata al testo, con priorità allaordinate
  per richieste di upload rispetto allo scan (due canali separati, o priorità fissa:
  prima la coda upload, poi quella scan);
- worker che chiama `extract_and_store` con `state.config.poppler_timeout` e l'BM
  server compare document entry snel senza `text_status`;
- il documento è subito apribile: il frontend mostra il testo quando arriva
  (il refresh successivo carica `text_status` aggiornato).

## File coinvolti

- `apps/server/src/state.rs` (coda + worker, come per le thumbnail)
- `apps/server/src/routes/documents/upload.rs` (rimozione chiamata inline)
- `apps/server/src/scanner/index.rs` (add/refresh)
- test: upload risponde prima dell'estrazione; il testo arriva in un secondo tempo

## Accettazione

- La risposta dell'upload non aspetta più `pdftotext`.
- `text_status` diventa `ok`/`empty`/`failed` entro il timeout senza azione dell'utente.
- Il recupero dell'apertura non mostra testo non ancora disponibile.
