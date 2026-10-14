# T15 — Hardening del compose d'esempio

- **Importanza**: Media
- **Effort**: S (< 1 giorno)
- **Dipendenze**: nessuna (T16 può rientrarci)

## Contesto

`docker/docker-compose.yml` d'esempio non applica le restrizioni standard per un
servizio esposto in LAN: il container gira con capability complete, filesystem
scrivibile ovunque e senza limiti di risorse.

## Proposta

Aggiungere al compose d'esempio (commentato dove può rompere setup particolari):

- `read_only: true` + `tmpfs` per `/tmp` (il server scrive solo in `/data` e `/library`)
- `cap_drop: [ALL]` (+ solo `CHOWN`/`SETUID`/`SETGID` se servono a `entrypoint.sh`/gosu)
- `security_opt: [no-new-privileges:true]`
- `mem_limit` / `cpus` conservativi, `pids_limit`
- `restart: unless-stopped`

Verificare che PUID/PGID-remap e backup funzionino con rootfs read-only.

## File coinvolti

- `docker/docker-compose.yml`
- `README.md` (nota operativa)
- `docker/entrypoint.sh` (solo se il remap richiede aggiustamenti)

## Accettazione

- `docker compose up` funzionante con le restrizioni attive (upload, scan, backup ok).
- Ogni voce aggressiva è commentata/documentata con il motivo.
