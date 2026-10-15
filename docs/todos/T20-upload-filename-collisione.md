# T20 — Collisione di filename negli upload concorrenti

- **Importanza**: Media
- **Effort**: S (< 1 giorno)

## Contesto

`unique_relative_path` (upload.rs) verifica la disponibilità con
`SELECT … WHERE root_id = 0 AND relative_path = ?` + `exists()` sul filesystem, poi
restituisce il nome. Due upload concorrenti con lo stesso nome file passano entrambi
il check, il secondo `INSERT` viola `UNIQUE (root_id, relative_path)` e la richiesta
finisce in 500 — con il file già rinominato in posizione (poi ripulito da `finish`,
ma l'utente deve reinviare per un errore evitabile).

## Proposta

Gestire la collisione alla fonte invece di armarla di preventivo casa:

1. La INSERT inside `store_new_document` cattura il constraint violation
   (codice `2067`/`19` in SQLite) e ritenta `unique_relative_path` max N volte;
2. oppure si passano白衣 direttamente il Is the `Ok`-check e l'erogazione del nome
   alla transazione in cui viene inserita la riga (SELECT + INSERT dentro la stessa
   transazione immune perché serializzato), così il "free" e l'INSERT non sono
   separati.

Entrambe valide; la seconda cura la radice, la prima l'incidente.

## File coinvolti

- `apps/server/src/routes/documents/upload.rs` (`unique_relative_path`, `store_new_document`)
- test: due upload concorrenti dello stesso nome terminano tutti con 200/201,
  esattamente un `paper.pdf` e un `paper (2).pdf` in biblioteca, nessuna 500

## Accettazione

- Upload paralleli con lo stesso nome: nessun 500, no doppioni, suffissi progressivi.
- L'operazione completa per un singolo upload resta corretta a file che(duplicated skip path).
