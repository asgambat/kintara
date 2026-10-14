# T01 — Concorrenza limitata nello scan iniziale

- **Importanza**: Alta
- **Effort**: M (1–3 giorni, inclusi test)
- **Dipendenze**: nessuna
- **Stato**: ✅ implementato (`scan_jobs`, `KINTARA_SCAN_JOBS`, `JoinSet` + `Semaphore` in `full_scan`)

## Contesto

`scanner::full_scan` (`apps/server/src/scanner/mod.rs`) indicizza i file **uno alla
volta**: per ogni PDF lancia in sequenza `blake3` + `pdfinfo` + `pdftoppm` +
`pdftotext`. Su NAS lenti e librerie con migliaia di PDF lo scan iniziale dura ore,
quasi tutto in attesa I/O e processi esterni.

## Proposta

Elaborare i file con concorrenza limitata (es. `tokio::Semaphore` con N = numero di
CPU / 2, configurabile via env), mantenendo l'ordine dei log e i contatori
`added/updated/removed`. Il reconcile finale (`seen` vs `known`) resta sequenziale.

## File coinvolti

- `apps/server/src/scanner/mod.rs` (`full_scan`)
- `apps/server/src/scanner/index.rs` ( invariato, già `async`)
- `apps/server/src/config.rs` (nuova env, es. `KINTARA_SCAN_JOBS`)
- `apps/server/tests/scanner.rs` (test di concorrenza sui contatori)

## Accettazione

- Scan di una fixture con N PDF impiega ~1/N del tempo (tolleranza per I/O).
- I contatori finali sono identici alla versione sequenziale.
- Nessun errore di borrow/race sotto `cargo test` ripetuto.
