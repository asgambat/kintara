# TODO — Ottimizzazioni e migliorie Kintara

Elenco generato dall'analisi statica del codice. Nessuna voce è ancora implementata.
Ogni file `Txx-*.md` è autonomo: importanza, effort, dipendenze, file coinvolti,
descrizione e criteri di accettazione.

Scala usata:
- **Importanza**: Alta / Media / Bassa (impatto su utenti, NAS lenti, librerie grandi).
- **Effort**: S (< 1 giorno), M (1–3 giorni), L (> 3 giorni o dipendenze esterne).
- **Dipendenze**: altri TODO da completare prima (o insieme).

## Backend — performance

| ID | Titolo | Importanza | Effort | Dipendenze |
|----|--------|------------|--------|------------|
| T01 | Concorrenza limitata nello scan iniziale ✅ | Alta | M | — |
| T02 | Scan di avvio in background ✅ | Alta | S | — (abilitava T12) |
| T03 | INSERT pagine in batch in `extract_and_store` | Media | S | — |
| T04 | Upload robusto (temp da crash + anti-orfani) ✅ | Alta | M | — (consigliato prima di T05) |
| T05 | Thumbnail in background con placeholder | Media | M | T04 |
| T06 | Timeout + kill sui processi poppler | Alta | S | — |

## Backend — database

| ID | Titolo | Importanza | Effort | Dipendenze |
|----|--------|------------|--------|------------|
| T07 | Indice su `library_documents(document_id)` | Media | S | — (migrazione) |
| T08 | `auto_vacuum` incrementale + manutenzione | Bassa | S | — |
| T09 | Snapshot schedulati del DB (oltre il pre-migrazione) | Media | S | — (riusa rotazione esistente) |

## Backend — robustezza / sicurezza

| ID | Titolo | Importanza | Effort | Dipendenze |
|----|--------|------------|--------|------------|
| T10 | Rescan periodico opzionale (watcher cieco su CIFS) | Media | S | — |
| T11 | Rate limiting sugli endpoint auth | Alta | M | — |

## Frontend

| ID | Titolo | Importanza | Effort | Dipendenze |
|----|--------|------------|--------|------------|
| T12 | `React.lazy` per readers e pannello AI ✅ | Media | S | — |
| T13 | Virtualizzazione della lista documenti ✅ | Alta | M | — |
| T14 | Cover con `decoding="async"` (lazy già presente) ✅ | Media | S | — |

## Docker / build / ops

| ID | Titolo | Importanza | Effort | Dipendenze |
|----|--------|------------|--------|------------|
| T15 | Hardening del compose d'esempio | Media | S | — |
| T16 | `max-size` sui log JSON di Docker | Bassa | S | — (sta in T15 se fatto insieme) |
| T17 | `Cache-Control: immutable` su `/assets/*` ✅ | Bassa | S | — |

Stato: T01, T02, T04, T12, T13, T14 e T17 implementati. Resto: T03, T05, T06, T07, T08, T09, T10, T11, T15, T16.
