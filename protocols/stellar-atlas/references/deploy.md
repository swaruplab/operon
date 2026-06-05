# STELLAR Deployment Patterns

`stellar serve` is for local development. For production (sharing with collaborators or hosting publicly), use one of the patterns below.

## Pattern 1 — Direct uvicorn behind nginx (simplest)

Suitable for: lab-internal sharing, single-atlas servers, modest traffic.

```
                    +---------+      +-----------+      +-------------+
   browser  ---->   |  nginx  | ---> |  uvicorn  | ---> |  LanceDB +  |
                    |  :443   |      |  :18901   |      |   DuckDB    |
                    +---------+      +-----------+      +-------------+
```

### systemd unit

```ini
# /etc/systemd/system/stellar-atlas.service
[Unit]
Description=STELLAR atlas server
After=network.target

[Service]
Type=simple
User=stellar
WorkingDirectory=/srv/stellar/my_atlas
Environment="ANTHROPIC_API_KEY=sk-..."
Environment="NCBI_EMAIL=you@uci.edu"
ExecStart=/srv/stellar/.venv/bin/stellar serve \
            --config /srv/stellar/my_atlas/stellar.yaml \
            --host 127.0.0.1 --port 18901
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=multi-user.target
```

### nginx config

```nginx
server {
  server_name atlas.example.edu;
  listen 443 ssl http2;

  ssl_certificate     /etc/letsencrypt/live/atlas.example.edu/fullchain.pem;
  ssl_certificate_key /etc/letsencrypt/live/atlas.example.edu/privkey.pem;

  # API + SPA both proxied to uvicorn
  location / {
    proxy_pass         http://127.0.0.1:18901;
    proxy_http_version 1.1;
    proxy_set_header   Host              $host;
    proxy_set_header   X-Real-IP         $remote_addr;
    proxy_set_header   X-Forwarded-For   $proxy_add_x_forwarded_for;
    proxy_set_header   X-Forwarded-Proto $scheme;

    # WebSocket / streaming responses (Copilot module)
    proxy_set_header   Upgrade           $http_upgrade;
    proxy_set_header   Connection        "upgrade";

    proxy_read_timeout 300s;       # for long Copilot chats
  }
}
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now stellar-atlas
sudo nginx -t && sudo systemctl reload nginx
```

## Pattern 2 — Sub-path mount (multiple atlases on one server)

When you want `https://lab.example.edu/atlas_a/` and `.../atlas_b/` served by two stellar processes.

### Run each atlas on a distinct port

```bash
stellar serve --config /srv/atlas_a/stellar.yaml --port 18901
stellar serve --config /srv/atlas_b/stellar.yaml --port 18902
```

### nginx

```nginx
server {
  server_name lab.example.edu;
  listen 443 ssl http2;

  location /atlas_a/ {
    proxy_pass http://127.0.0.1:18901/atlas_a/;
    # same proxy_set_header lines as Pattern 1
  }

  location /atlas_b/ {
    proxy_pass http://127.0.0.1:18902/atlas_b/;
    # same proxy_set_header lines as Pattern 1
  }

  # /api is reverse-proxied per-atlas because each backend serves its own
  location /atlas_a/api/ { proxy_pass http://127.0.0.1:18901/api/; }
  location /atlas_b/api/ { proxy_pass http://127.0.0.1:18902/api/; }
}
```

In each `stellar.yaml`, set `project.name` to match the URL path (`atlas_a`, `atlas_b`). The SPA's internal routing handles the rest.

## Pattern 3 — Gunicorn workers (heavier traffic)

Replace the bare `stellar serve` with gunicorn for multi-worker concurrency.

```bash
# Find the FastAPI app — it's stellar.api.app:app or similar
gunicorn stellar.api.app:app \
  --workers 4 \
  --worker-class uvicorn.workers.UvicornWorker \
  --bind 127.0.0.1:18901 \
  --timeout 120
```

systemd unit changes the `ExecStart` line; the nginx config stays the same.

Note: LanceDB and DuckDB connections are per-worker — for very large atlases on small RAM servers, stick with a single worker.

## Pattern 4 — Docker (portable)

A minimal Dockerfile:

```dockerfile
FROM python:3.12-slim AS base
RUN apt-get update && apt-get install -y --no-install-recommends \
      r-base r-base-dev libcurl4-openssl-dev libxml2-dev libssl-dev \
    && rm -rf /var/lib/apt/lists/*
RUN R -e "install.packages(c('Seurat', 'SeuratDisk'), repos='https://cloud.r-project.org')"

WORKDIR /app
COPY . .
RUN pip install --no-cache-dir 'stellar-atlas[full]'

EXPOSE 18901
CMD ["stellar", "serve", "--config", "/app/stellar.yaml", \
     "--host", "0.0.0.0", "--port", "18901"]
```

```bash
docker build -t my_atlas .
docker run -d --name my_atlas \
  -p 18901:18901 \
  -e ANTHROPIC_API_KEY=sk-... \
  -v /data/atlas:/app/data:ro \
  my_atlas
```

## Production checklist

- [ ] `stellar doctor` returns 0 issues
- [ ] systemd unit `Restart=on-failure` set
- [ ] nginx has SSL (Let's Encrypt or institutional cert)
- [ ] `ANTHROPIC_API_KEY` (if Copilot enabled) is in the systemd unit's `Environment=`, **not** in `stellar.yaml` checked into git
- [ ] LanceDB / DuckDB / parquet files are on fast storage (NVMe or local SSD — NFS works but adds latency)
- [ ] Reverse-proxy `proxy_read_timeout` ≥ 300s so Copilot streaming responses don't get cut off
- [ ] Backup strategy in place — the `data/lance/` and `data/parquet/` directories ARE your atlas; back them up alongside `stellar.yaml`

## Updating an existing atlas

When the underlying data changes (new cells added, DE results recomputed, etc.):

```bash
# On the server, with the service stopped
sudo systemctl stop stellar-atlas

# Re-ingest in the project directory
stellar ingest --config stellar.yaml
stellar doctor --config stellar.yaml

# Restart
sudo systemctl start stellar-atlas
```

For zero-downtime updates, ingest into a parallel directory then swap with a symlink rename:

```bash
stellar ingest --config /srv/my_atlas_new/stellar.yaml
stellar doctor --config /srv/my_atlas_new/stellar.yaml
sudo ln -sfn /srv/my_atlas_new /srv/my_atlas_current
sudo systemctl reload stellar-atlas
```
