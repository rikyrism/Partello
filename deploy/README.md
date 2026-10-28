# Meng-online-kan Partello di satu VPS

Panduan ini membawa Partello dari laptop ke internet supaya teman bisa bergabung dari perangkat dan jaringan mana pun: HTTPS otomatis, WebSocket aman (`wss://`), database yang tidak terbuka ke internet, dan server TURN agar voice/video tetap tersambung di jaringan seluler.

## Ukuran VPS

Yang membuat Partello ringan: **video YouTube tidak lewat VPS**. Setiap penonton mengambil video langsung dari YouTube; VPS hanya mengirim pesan sinkronisasi dan chat yang kecil. Voice/video call juga langsung antar-perangkat (peer-to-peer). VPS baru ikut mengalirkan media saat sebuah koneksi **harus** lewat TURN, misalnya teman di jaringan seluler (CGNAT) atau Wi-Fi kantor/kampus yang ketat.

| Kebutuhan | Spesifikasi | Cocok untuk |
| --- | --- | --- |
| **Mulai / teman-teman** (rekomendasi awal) | **1–2 vCPU, 2 GB RAM, 25 GB SSD, kuota transfer ≥ 1 TB/bulan** | Beberapa room aktif bersamaan, puluhan orang online |
| Komunitas | 2 vCPU, 4 GB RAM, 40–80 GB SSD, ≥ 2–3 TB/bulan, port 1 Gbps | Puluhan room aktif, ratusan orang online, banyak yang call pakai kamera |
| Lebih besar | Pindahkan TURN ke VPS sendiri atau layanan TURN terkelola, gunakan PostgreSQL terkelola | Versi ini masih satu instance (state room di memori) |

Pemakaian memori kira-kira: aplikasi Rust 30–80 MB, PostgreSQL 100–250 MB, Caddy 30–50 MB, coturn ±20 MB. **2 GB RAM** dipilih agar proses build image (kompilasi Rust) tidak kehabisan memori. Jika memakai VPS 1 GB, tambahkan swap 2 GB (lihat langkah 2) atau build image di komputer lain.

Perkiraan bandwidth TURN: dalam call 4 orang dengan kamera, satu orang yang harus lewat TURN mengalirkan ±3 Mbps lewat VPS (±1,3 GB per jam). Audio saja jauh lebih kecil (±0,2 Mbps). Kuota 1 TB/bulan biasanya lebih dari cukup untuk pemakaian pribadi.

**Lokasi server:** pilih **Jakarta** atau **Singapura** agar dekat dengan pengguna di Indonesia (latensi rendah membuat sinkronisasi dan call terasa instan). Penyedia lokal maupun global yang punya region Singapura sama-sama bisa dipakai, asalkan:

- menyediakan IPv4 publik;
- mengizinkan port UDP (dibutuhkan TURN);
- mendukung Docker (Ubuntu 22.04/24.04 LTS disarankan).

## Yang perlu disiapkan

1. VPS dengan Ubuntu 22.04/24.04 dan akses SSH.
2. **Domain** (atau subdomain), misalnya `nonton.domainanda.com`. HTTPS wajib: tanpa HTTPS, browser menolak akses mikrofon/kamera.
3. (Opsional) API key YouTube Data API v3 untuk fitur pencarian.

## Langkah

### 1. Arahkan domain ke VPS

Di pengelola DNS domain Anda, buat **A record** `nonton.domainanda.com` → IP publik VPS. Tunggu sampai `ping nonton.domainanda.com` menunjukkan IP tersebut.

### 2. Siapkan server

```bash
ssh root@IP_VPS

# Docker + Compose
curl -fsSL https://get.docker.com | sh

# Firewall: SSH, web, dan TURN
ufw allow 22/tcp
ufw allow 80/tcp
ufw allow 443/tcp
ufw allow 443/udp
ufw allow 3478/tcp
ufw allow 3478/udp
ufw allow 49152:49352/udp
ufw enable

# Hanya untuk VPS dengan RAM 1 GB: swap 2 GB untuk proses build
fallocate -l 2G /swapfile && chmod 600 /swapfile && mkswap /swapfile && swapon /swapfile
echo '/swapfile none swap sw 0 0' >> /etc/fstab
```

Jika penyedia VPS punya firewall sendiri (security group / cloud firewall), buka port yang sama di sana juga.

### 3. Ambil kode dan isi konfigurasi

```bash
git clone <URL-repo-Anda> partello && cd partello
cp deploy/.env.prod.example deploy/.env
nano deploy/.env
```

Isi `DOMAIN`, `PUBLIC_IP`, lalu buat dua rahasia acak:

```bash
openssl rand -base64 32   # untuk POSTGRES_PASSWORD
openssl rand -base64 32   # untuk TURN_SHARED_SECRET
```

### 4. Jalankan

```bash
docker compose -f deploy/docker-compose.prod.yml --env-file deploy/.env up -d --build
```

Build pertama memakan waktu beberapa menit (kompilasi Rust). Caddy otomatis mengambil sertifikat HTTPS saat pertama diakses.

### 5. Periksa

```bash
curl https://nonton.domainanda.com/healthz   # {"status":"ok",...}
curl https://nonton.domainanda.com/readyz    # {"status":"ready"}
docker compose -f deploy/docker-compose.prod.yml --env-file deploy/.env ps
```

Lalu uji dari dua perangkat berbeda, idealnya satu di **data seluler** dan satu di Wi-Fi: buat room, kirim undangan, play/pause, chat, dan gabung call. Daftar skenario lengkap ada di `docs/TESTING.md`.

Untuk menguji TURN: buka https://webrtc.github.io/samples/src/content/peerconnection/trickle-ice/ dan isi `turn:nonton.domainanda.com:3478`. Username dan password bisa dibuat dengan rumus di `docs/DEPLOYMENT.md`. Kandidat bertipe `relay` berarti TURN bekerja.

## Perawatan

**Update ke versi baru**

```bash
git pull
docker compose -f deploy/docker-compose.prod.yml --env-file deploy/.env up -d --build
```

Restart memutus koneksi sebentar. Klien tersambung ulang otomatis, tetapi antrean dan posisi video di room yang sedang aktif akan hilang. Lakukan saat sepi.

**Backup database** (room, chat, API key), misalnya harian lewat cron:

```bash
docker compose -f deploy/docker-compose.prod.yml --env-file deploy/.env exec -T postgres \
  pg_dump -U partello partello | gzip > ~/partello-$(date +%F).sql.gz
```

**Log**

```bash
docker compose -f deploy/docker-compose.prod.yml --env-file deploy/.env logs -f app
```

## Supaya teman bisa join tanpa kendala

- **Gunakan link `https://` dari tombol Undang.** Link `localhost` hanya bisa dibuka di komputer Anda sendiri.
- **TURN wajib di Indonesia.** Banyak operator seluler memakai CGNAT; tanpa TURN, sebagian teman bisa menonton dan chat tetapi **tidak tersambung voice/video**. Paket ini sudah menyertakannya.
- **iPhone/Android:** browser kadang menahan suara video sampai layar disentuh sekali; Partello menampilkan tombol "Nyalakan suara".
- **Memakai Cloudflare (proxy oranye) di depan VPS?** Tambahkan `CLIENT_IP_HEADER: CF-Connecting-IP` di service `app`, dan aktifkan dukungan WebSocket di Cloudflare. Tanpa header itu, semua pengguna terlihat sebagai IP Cloudflare dan bisa saling terkena rate limit. Catatan: TURN tidak bisa lewat proxy Cloudflare, jadi buat record DNS terpisah (misalnya `turn.domainanda.com`, proxy abu-abu) dan pakai itu di `TURN_URLS`.
- **Satu jaringan rumah/kafe** berbagi satu IP. Batas percobaan PIN (10 per 5 menit per IP) cukup longgar untuk satu keluarga, tetapi jangan bagikan PIN yang salah.
- **Maksimal 16 orang per room, 8 orang dalam call.** Batas ini bisa diubah lewat `ROOM_MAX_PARTICIPANTS` / `ROOM_MAX_VOICE`. Call adalah mesh peer-to-peer, jadi lebih dari 8 orang berkamera akan berat untuk ponsel.
