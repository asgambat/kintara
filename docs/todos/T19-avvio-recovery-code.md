# T19 — Recovery all'avvio di test e thumbnail mancanti

- **Importanza**: Media
- **Effort**: S (< 1 giorno)
- **Dipendenze**: T05 (il worker già esiste)

## Contesto

Le code di generazione (thumbnail da T05, testo da T18) vive **solo in memoria**: un
riavvio mentre la coda ha lavoro pendente lo perde. Un documento caricato un secondo
prima del crash rimane privo di cover e senza `text_status` per sempre — lo scan di
avvio non reinserisce perché l'hash non è cambiato.

## Proposta

All'avvio, dopo `purge_expired_sessions`, una query unica prende i candidati:

```sql
SELECT id, root_id, relative_path FROM documents
WHERE document_type = 'pdf'
  AND (thumbnail_name IS NULL OR text_status IS NULL);
```

e li ri-accoda: thumbnail_name NULL → job sulla coda thumbnail;
`text_status IS NULL` (o `text_status = 'failed'` opzionale sopra flag) → job di testo.
Il cleanup dei job duplicati non serve: gli update condizionati del worker già
gestiscono il "stale" (`thumbnail_name IS ?`).
Se si introduce, si valuta anche un limite di tentativi memorable? no: la recovery
gira una volta per avvio, è naturalmente limitata.

## File coinvolti

- `apps/server/src/main.rs` (task di recovery all'avvio)
- `apps/server/src/state.rs` (coda già pronta)
- test: document con cover mancante ma hash invariato viene completato al riavvio

## Accettazione

- Dopo il riavvio, i documenti `pdf` senza cover e senza `text_status` ottengono
  entrambi i risultati senza toccare i file.
- Nessun job duplicato che sovrascrive una cover personalizzata (guardia già in
  `process_thumbnail_job`).
- Avvio più coal nel caso esterno: la recovery non blocca il bind della socket.
