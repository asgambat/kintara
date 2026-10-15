# T21 — Allineare il compose d'esempio all'immagine pubblicata

- **Importanza**: Alta
- **Effort**: S (< un'ora)

## Contesto

`docker/docker-compose.yml` apre con

```yaml
image: ghcr.io/pinkpixel-dev/kintara:latest
```

ma il workflow di CI pubblica l'immagine in **ghcr.io/asgambat/kintara:latest**.
Chi copia il compose d'esempio riceve `Image pull error … unauthorized` — esattamente
il sintomo che ha bloccato l'installazione reale di chi mantiene il progetto (pacchetto
privato/mancante sotto un namespace che non publichiamo).

## Proposta

- Sostituire `image:` con `ghcr.io/asgambat/kintara:latest` (o, meglio, con il suo
  valore parametrico in `.env`: `image: ${KINTARA_IMAGE:-ghcr.io/asgambat/kintara:latest}`).
- Rileggere `README.md` e gli altri file reference per eventuale menzione della vecchia
  immagine (`grep -rn "pinkpixel" .` come checklist).
- Valutare, senza impegno, di nota nella sezione Install del README che l'immagine è
  pubblica (segue da T05/T21 dopo la prima pubblicazione verde).

## File coinvolti

- `docker/docker-compose.yml`
- `README.md` (checklist `pinkpixel`)

## Accettazione

- `grep -rn pinkpixel docker/ README.md` → 0 risultati.
- Un `docker compose config` in `docker/` mostra l'immagine ci pubblica e il tag `latest`.
