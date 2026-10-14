# T04 — Upload in streaming su temp file (non in RAM)

- **Importanza**: Alta
- **Effort**: M (1–3 giorni, inclusi test di integrazione)
- **Dipendenze**: nessuna (consigliato prima di T05)
- **Stato**: ✅ parzialmente implementato (vedi sotto)

## Contesto

Verifica sul codice: l'upload documenti era **già** in streaming (`StreamedUpload`:
chunk → temp file in `.kintara-incoming/` + hash on the fly + rename atomico). Il
`bytes: Option<Vec<u8>>` visto in analisi è di `upload_cover`, limitato a 32 MB da
`DefaultBodyLimit` — buffering voluto e innocuo. Restavano due buchi, ora chiusi:

1. i temp `.upload-*` di upload interrotti da un crash restavano per sempre;
2. se lo store nel DB falliva dopo il rename, restava un file senza riga.

## Proposta

Streaming già presente (nessuna modifica). Implementato:

1. `discard_stale_uploads()` in `upload.rs`, chiamata in `main` dopo
   `ensure_dirs()`: rimuove i `.upload-*` all'avvio (solo prefisso temp, tutto il
   resto preservato).
2. `finish()` scorporata: `store_new_document()` + rimozione del file piazzato se
   lo store fallisce (niente più orfani senza riga).

## File coinvolti

- `apps/server/src/routes/documents/upload.rs`
- `apps/server/tests/uploads.rs` (nuovo test cleanup; i test esistenti coprono streaming e limite)
- `apps/server/src/main.rs` (chiamata all'avvio), `routes/documents/mod.rs` (`pub mod upload`)

## Accettazione

- I test esistenti (5 MB > DefaultBodyLimit, nessun temp residuo) restano verdi.
- Nuovo test: temp da crash rimosso, `stray.pdf` preservato.
- Store fallito → nessun file orfano nella root primaria.
