# T10 — Rescan periodico opzionale (watcher cieco su CIFS)

- **Importanza**: Media
- **Effort**: S (< 1 giorno)
- **Dipendenze**: nessuna

## Contesto

`spawn_watcher` (`scanner/mod.rs`) usa `notify`/inotify, che **non rileva modifiche
avvenute dal lato server di un mount CIFS/SMB client**. Se `/library` è un mount di
rete (non un bind di una dir locale che fa da server SMB), il watcher resta cieco e
i nuovi file appaiono solo al restart.

## Proposta

Env `KINTARA_RESCAN_INTERVAL_HOURS` (0 = disabilitato): un task schedulato lancia
`full_scan` a intervalli. `full_scan` è già idempotente (hash → skip), quindi il
costo a regime è solo hashing. Combinabile con T01 (scan concorrente) per rescan
veloci.

## File coinvolti

- `apps/server/src/main.rs` (scheduler)
- `apps/server/src/config.rs` (nuova env)
- `README.md` (quando serve: mount CIFS vs bind locale)

## Accettazione

- Con watcher disabilitato e rescan a N ore, un file aggiunto appare entro N ore.
- A regime (nulla cambiato) il rescan non scrive sul DB (solo `Skipped`).
