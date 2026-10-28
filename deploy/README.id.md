# Meng-online-kan Partello di satu VPS

[English](README.md) · Bahasa Indonesia

Panduan ini membawa Partello dari laptop ke internet supaya teman bisa bergabung dari perangkat dan jaringan mana pun: HTTPS otomatis, WebSocket aman (`wss://`), database yang tidak terbuka ke internet, dan server TURN agar voice/video tetap tersambung di jaringan seluler. SFU untuk call yang lebih besar bersifat opsional.

## Ukuran VPS

Yang membuat Partello ringan: **video YouTube tidak lewat VPS**. Setiap penonton mengambil video langsung dari YouTube; VPS hanya mengirim pesan sinkronisasi dan chat yang kecil. Voice/video call juga langsung antar-perangkat (peer-to-peer). VPS baru ikut mengalirkan media saat sebuah koneksi **harus** lewat TURN, misalnya teman di jaringan seluler (CGNAT) atau Wi-Fi kantor/kampus yang ketat, atau saat SFU opsional diaktifkan (lihat [Call besar dengan SFU](#call-besar-dengan-sfu-opsional)).

| Kebutuhan | Spesifikasi | Cocok untuk |
| --- | --- | --- |
| **Mulai / teman-teman** (rekomendasi awal) | **1–2 vCPU, 2 GB RAM, 25 GB SSD, kuota transfer ≥ 1 TB/bulan** | Beberapa room aktif bersamaan, puluhan orang online |
| Komunitas | 2 vCPU, 4 GB RAM, 40–80 GB SSD, ≥ 2–3 TB/bulan, port 1 Gbps | Puluhan room aktif, ratusan orang online, banyak yang call pakai kamera |
| Lebih besar | Pindahkan TURN ke VPS sendiri atau layanan TURN terkelola, gunakan PostgreSQL terkelola | Versi ini masih satu instance (sesi ada di memori) |

Pemakaian memori kira-kira: aplikasi Rust 30–80 MB, PostgreSQL 100–250 MB, Caddy 30–50 MB, coturn ±20 MB. **2 GB RAM** dipilih agar proses build image (kompilasi Rust) tidak kehabisan memori. Jika memakai VPS 1 GB, tambahkan swap 2 GB (lihat langkah 2) atau build image di komputer lain.

Perkiraan bandwidth TURN: dalam call 4 orang dengan kamera, satu orang yang harus lewat TURN mengalirkan ±3 Mbps lewat VPS (±1,3 GB per jam). Audio saja jauh lebih kecil (±0,2 Mbps). Kuota 1 TB/bulan biasanya lebih dari cukup untuk pemakaian pribadi.

**Lokasi server:** pilih **Jakarta** atau **Singapura** agar dekat dengan pengguna di Indonesia (latensi rendah membuat sinkronisasi dan call terasa instan). Penyedia lokal maupun global yang punya region Singapura sama-sama bisa dipakai, asalkan:

- menyediakan IPv4 publik;
- mengizinkan port UDP (dibutuhkan TURN);
- mendukung Docker (Ubuntu 22.04/24.04 LTS disarankan).

## Yang perlu disiapkan

1. VPS dengan Ubuntu 22.04/24.04 dan akses SSH.
2. **Domain** (atau subdomain), misalnya `nonton.domainanda.com`. HTTPS wajib: tanpa HTTPS, browser menolak akses mikrofon/kamera, pemasangan aplikasi, dan notifikasi push.
3. (Opsional) API key YouTube Data API v3 untuk fitur pencarian dan impor playlist.

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
# Hanya jika memakai SFU opsional (--profile sfu). JANGAN buka 7880.
# ufw allow 7881/tcp
# ufw allow 50100:50300/udp
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

Lalu uji dari dua perangkat berbeda, idealnya satu di **data seluler** dan satu di Wi-Fi: buat room, kirim undangan, play/pause, chat, dan gabung call. Daftar skenario lengkap ada di [`docs/id/TESTING.md`](../docs/id/TESTING.md).

Untuk menguji TURN: buka https://webrtc.github.io/samples/src/content/peerconnection/trickle-ice/ dan isi `turn:nonton.domainanda.com:3478`. Username dan password bisa dibuat dengan rumus di [`docs/id/DEPLOYMENT.md`](../docs/id/DEPLOYMENT.md#4a-coturn). Kandidat bertipe `relay` berarti TURN bekerja.

## Call besar dengan SFU (opsional)

Tanpa SFU, setiap ponsel dalam call mengirim kameranya ke setiap ponsel lain, dan ini menjadi berat di atas sekitar 8 kamera. Dengan server LiveKit yang sudah disertakan, setiap perangkat cukup mengirim kameranya sekali ke VPS dan LiveKit meneruskannya ke yang lain. Call bisa menampung semua orang di room (`ROOM_MAX_VOICE` default-nya menjadi `ROOM_MAX_PARTICIPANTS`, bukan 8).

1. Tambahkan ke `deploy/.env`:

   ```
   LIVEKIT_URL=wss://nonton.domainanda.com
   LIVEKIT_API_URL=http://host.docker.internal:7880
   LIVEKIT_API_KEY=partello                  # nama bebas
   LIVEKIT_API_SECRET=<openssl rand -base64 32>
   ```

2. Buka port SFU: `ufw allow 7881/tcp` dan `ufw allow 50100:50300/udp` (juga di firewall penyedia). **Jangan** buka 7880: Caddy menjangkaunya dari dalam VPS.
3. Jalankan dengan profile `sfu`:

   ```bash
   docker compose -f deploy/docker-compose.prod.yml --env-file deploy/.env --profile sfu up -d --build
   ```

Caddy meneruskan `/rtc` di domain yang sama ke LiveKit, jadi tidak perlu record DNS atau sertifikat tambahan. Secret LiveKit tetap di server; browser hanya menerima token berumur pendek untuk satu call.

**Ukuran VPS dengan SFU.** Media call sekarang mengalir lewat VPS. Sebagai perkiraan kasar, setiap kamera yang dikirim memakan sekitar 0,5–1,5 Mbps masuk, lalu diteruskan ke setiap orang yang menontonnya. Call 8 orang yang semuanya menyalakan kamera bisa mengalirkan puluhan Mbps keluar dari VPS. Untuk call 8 orang atau lebih yang banyak kameranya, siapkan minimal 2 vCPU / 4 GB RAM dan paket dengan bandwidth lebih besar, lalu ukur pemakaian Anda sendiri.

**Tidak mau SFU di VPS?** LiveKit Cloud punya paket gratis: isi URL `wss://`, API key, dan secret-nya di `LIVEKIT_URL` / `LIVEKIT_API_KEY` / `LIVEKIT_API_SECRET`, biarkan `LIVEKIT_API_URL` kosong, dan jalankan tanpa `--profile sfu`.

## Perawatan

**Update ke versi baru**

```bash
git pull
docker compose -f deploy/docker-compose.prod.yml --env-file deploy/.env up -d --build
```

(Tambahkan `--profile sfu` jika memakai SFU bawaan.)

Restart memutus koneksi sebentar; semua orang tersambung ulang dan bergabung kembali otomatis dalam beberapa detik. Video yang sedang diputar dan antrean setiap room tersimpan di database, jadi tidak ada yang hilang: video yang sedang diputar akan lanjut jika aplikasi kembali dalam 2 menit, jika lebih lama video menunggu dalam keadaan pause di posisi yang sama.

**Backup database** (room, chat, antrean tersimpan, API key, subscription notifikasi beserta key yang mengikatnya), misalnya harian lewat cron:

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
- **TURN wajib di Indonesia.** Banyak operator seluler memakai CGNAT; tanpa TURN, sebagian teman bisa menonton dan chat tetapi **tidak tersambung voice/video**. Paket ini sudah menyertakan coturn. TURN terkelola (Cloudflare Realtime TURN, Metered.ca) untuk setup tanpa coturn sendiri, misalnya tunnel; lihat [`docs/id/DEPLOYMENT.md`](../docs/id/DEPLOYMENT.md#4-turn).
- **iPhone/Android:** browser kadang menahan suara video sampai layar disentuh sekali; Partello menampilkan tombol "Nyalakan suara".
- **Pasang seperti aplikasi.** Di Android dan Chrome/Edge desktop, landing page menampilkan "Pasang aplikasi"; di iPhone/iPad pakai Bagikan → Tambah ke Layar Utama. Mode bioskop (tombol layar penuh di bawah video) tetap menampilkan chat, reaksi, dan call di atas video, cocok untuk ponsel.
- **Notifikasi "ada yang mulai nonton".** Lonceng di header room memberi tahu perangkat itu saat ada yang mulai nonton di room, bahkan saat Partello tertutup. Tidak ada yang perlu diatur: server membuat key notifikasinya saat pertama kali menyala dan menyimpannya di database (jadi pulihkan dari backup alih-alih mulai dari database kosong, atau semua orang harus menekan lonceng lagi). Di iPhone/iPad hanya bisa dari aplikasi yang ditambahkan ke Layar Utama (iOS 16.4 atau lebih baru). Tidak ada notifikasi dalam 2 menit pertama setelah restart, dan paling banyak satu per room setiap 10 menit.
- **Memakai Cloudflare (proxy oranye) di depan VPS?** Tambahkan `CLIENT_IP_HEADER: CF-Connecting-IP` di service `app`, dan aktifkan dukungan WebSocket di Cloudflare. Tanpa header itu, semua pengguna terlihat sebagai IP Cloudflare dan bisa saling terkena rate limit. Catatan: TURN dan port media SFU tidak bisa lewat proxy Cloudflare, jadi buat record DNS terpisah (misalnya `turn.domainanda.com`, proxy abu-abu) dan pakai itu di `TURN_URLS`.
- **Satu jaringan rumah/kafe** berbagi satu IP. Batas percobaan PIN (10 per 5 menit per IP) cukup longgar untuk satu keluarga, tetapi jangan bagikan PIN yang salah.
- **Room yang ingin disimpan:** centang "Simpan room ini selamanya" di pengaturan room. Room lain dihapus setelah tidak aktif selama `ROOM_RETENTION_DAYS` (default 30 hari).
- **Maksimal 16 orang per room, 8 orang dalam call.** Batas ini bisa diubah lewat `ROOM_MAX_PARTICIPANTS` / `ROOM_MAX_VOICE`. Tanpa SFU, call adalah mesh peer-to-peer, jadi lebih dari 8 orang berkamera akan berat untuk ponsel. Dengan SFU, batas call default-nya semua orang di room.
