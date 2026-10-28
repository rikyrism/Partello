# Panduan Deployment

> **Cara tercepat:** ikuti [`deploy/README.md`](../deploy/README.md). Satu perintah menjalankan Caddy (HTTPS otomatis), aplikasi, PostgreSQL, dan coturn di satu VPS, lengkap dengan rekomendasi ukuran VPS. Dokumen ini menjelaskan detail dan opsi lainnya.

Partello dikirim sebagai **satu image Docker** yang berisi binary Rust dan hasil build frontend. Kebutuhan lainnya: PostgreSQL dan, untuk voice/video yang andal, server TURN.

## 1. Build image

```bash
docker build -t partello:latest .
```

Image akhir berbasis `debian:bookworm-slim`, berjalan sebagai user non-root, dan memiliki `HEALTHCHECK` bawaan (`partello-server --healthcheck`).

## 2. Konfigurasi wajib produksi

| Variabel | Contoh | Catatan |
| --- | --- | --- |
| `DATABASE_URL` | `postgres://user:pass@db:5432/partello?sslmode=require` | Migrasi berjalan otomatis saat start |
| `ALLOWED_ORIGINS` | `https://partello.app` | Dipakai CORS **dan** cek Origin WebSocket. Jangan pakai `*` |
| `PUBLIC_BASE_URL` | `https://partello.app` | Untuk link undangan di respons API |
| `TRUST_PROXY` | `true` | Hanya jika di belakang proxy tepercaya (rate limit memakai `X-Forwarded-For`) |
| `LOG_FORMAT` | `json` | Untuk agregasi log |
| `STUN_URLS` / `TURN_URLS` / `TURN_SHARED_SECRET` | lihat bawah | Voice & video |
| `YOUTUBE_API_KEY` | – | Opsional, untuk pencarian. Simpan sebagai secret |

Semua secret disuntikkan sebagai environment variable atau secret manager, **bukan** dimasukkan ke image atau repository.

## 3. HTTPS dan WebSocket

Letakkan reverse proxy (Caddy, nginx, Traefik, atau load balancer cloud) di depan port 8080:

- TLS wajib: WebRTC (`getUserMedia`) hanya bekerja di origin aman, dan WebSocket harus `wss://`.
- Teruskan upgrade WebSocket untuk `/api/rooms/*/ws` dengan timeout idle ≥ 75 detik (server mengirim ping setiap 20 detik).

Contoh Caddy:

```
partello.app {
  reverse_proxy app:8080
}
```

Contoh nginx (potongan):

```nginx
location / {
  proxy_pass http://app:8080;
  proxy_http_version 1.1;
  proxy_set_header Upgrade $http_upgrade;
  proxy_set_header Connection $connection_upgrade;
  proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
  proxy_read_timeout 120s;
}
```

## 4. TURN (coturn)

STUN saja tidak cukup untuk NAT simetris dan jaringan kantor/kampus. Jalankan coturn dengan `use-auth-secret`:

```
listening-port=3478
tls-listening-port=5349
fingerprint
use-auth-secret
static-auth-secret=<secret-panjang-acak>
realm=partello.app
cert=/etc/ssl/turn.pem
pkey=/etc/ssl/turn.key
```

Lalu set di Partello:

```
TURN_URLS=turn:turn.partello.app:3478?transport=udp,turns:turn.partello.app:5349?transport=tcp
TURN_SHARED_SECRET=<secret yang sama>
```

Server mencetak kredensial yang berlaku `TURN_CREDENTIAL_TTL_SECS` (default 6 jam) untuk setiap peserta yang sudah terautentikasi. Shared secret tidak pernah dikirim ke browser.

Untuk menguji TURN secara manual, buat kredensial dengan rumus yang sama:

```bash
SECRET='isi TURN_SHARED_SECRET'
USERNAME="$(( $(date +%s) + 3600 )):test"
PASSWORD=$(printf '%s' "$USERNAME" | openssl dgst -sha1 -hmac "$SECRET" -binary | base64)
echo "$USERNAME / $PASSWORD"
```

Untuk pengujian lokal tersedia `docker compose --profile turn up -d coturn`.

## 5. Health check

- `GET /healthz`: proses hidup (liveness).
- `GET /readyz`: database terjangkau (readiness); mengembalikan 503 jika tidak.

## 6. Skala

Versi ini dirancang untuk **satu instance** (state room in-memory). Untuk lebih dari satu instance:

1. Gunakan sticky routing berdasarkan path `/r/{slug}` dan `/api/rooms/{slug}/…` agar satu room selalu berada di instance yang sama, atau
2. Ikuti jalur migrasi di [ARCHITECTURE.md](ARCHITECTURE.md#skala-dan-state-terdistribusi) (state & fan-out di Redis).

Restart (deploy) memutus semua koneksi. Klien tersambung ulang dan bergabung otomatis, tetapi antrean dan posisi playback room yang sedang aktif akan hilang. Lakukan deploy di jam sepi atau tambahkan persistensi state room terlebih dahulu.

## 7. Checklist sebelum go-live

- [ ] HTTPS + `wss://` aktif, `ALLOWED_ORIGINS` tepat
- [ ] TURN berjalan dan diuji dari jaringan seluler
- [ ] Backup PostgreSQL terjadwal
- [ ] Log JSON dikirim ke sistem observability; alert untuk `/readyz`
- [ ] Rate limit di edge/proxy (rate limit aplikasi per instance)
- [ ] Uji beban WebSocket untuk target jumlah room bersamaan
- [ ] Review keamanan (lihat keterbatasan di README)
