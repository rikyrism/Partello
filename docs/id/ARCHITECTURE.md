# Arsitektur Partello

[English](../ARCHITECTURE.md) · Bahasa Indonesia

## Gambaran umum

```
Browser (React)                                Server (Rust, satu proses)
┌──────────────────────────────┐   HTTPS     ┌────────────────────────────────────────┐
│ Landing / Room / Developers  │────────────▶│ routes/  REST + static SPA             │
│                              │             │   ├─ rooms.rs      (web app)           │
│ RoomController               │  WebSocket  │   └─ developer.rs  (API v1)            │
│  ├─ RoomSocket (auth, ping,  │◀───────────▶│ realtime/ws.rs   1 task per koneksi    │
│  │   reconnect, jam server)  │             │ realtime/hub.rs  room hidup + sesi     │
│  └─ store (reducer murni)    │             │ room/state.rs    aturan murni (no IO)  │
│                              │             │ database.rs      PostgreSQL            │
│ SyncedPlayer ◀── YouTube IFrame Player API │ youtube*.rs      oEmbed + Data API     │
│ MeshCall ◀──── WebRTC P2P ───▶ peer lain   │ ice.rs           kredensial TURN       │
│ SfuCall  ◀──── WebRTC ───▶ LiveKit (ops.)  │ sfu.rs           token/admin LiveKit   │
│ sw.js (PWA) ◀── layanan push browser ◀──   │ push.rs          Web Push (VAPID)      │
└──────────────────────────────┘             └────────────────────────────────────────┘
```

**Modular monolith**: satu binary Rust menyajikan REST, WebSocket, dan hasil build frontend. Tidak ada microservice dan tidak ada Redis. Keduanya baru masuk akal setelah ada kebutuhan multi-instance yang konkret. Satu-satunya komponen eksternal opsional adalah SFU LiveKit untuk call besar.

## Backend

| Modul | Tanggung jawab |
| --- | --- |
| `config` | Membaca environment variable; ringkasan log tanpa secret |
| `room::state` | **Seluruh aturan room**: presence, host, izin, playback, antrean, vote skip, chat, reaksi, voice. Murni, tanpa IO. Setiap operasi mengembalikan daftar `Effect` (broadcast, kirim ke satu orang, simpan chat) |
| `room::playback` | State playback otoritatif: `position` pada `updated_at` + `status` + `version` |
| `room::queue` | Antrean "up next" |
| `realtime::hub` | Memiliki room yang sedang hidup dan sesi; menerjemahkan `Effect` ke frame WebSocket dan penulisan database; menyimpan state room |
| `realtime::ws` | Satu task per koneksi: auth frame pertama, rate limit per jenis event, heartbeat, idle timeout |
| `realtime::protocol` | Enum event klien/server bertipe (serde, `type` sebagai tag) |
| `routes` | Handler HTTP tipis: DTO request/response eksplisit, logika ada di hub/state |
| `database` | `Store` = enum `Postgres | Memory`, sehingga test end-to-end berjalan tanpa database |
| `youtube` | Parser URL video dan URL playlist YouTube, oEmbed untuk judul/thumbnail sekaligus cek embeddability |
| `youtube_search` | YouTube Data API v3 (opsional), hanya di server, dengan cache; juga mengambil daftar video playlist |
| `ice` | ICE server untuk call: STUN plus TURN dari Cloudflare, coturn, atau kredensial statis |
| `sfu` | LiveKit (opsional): menandatangani token join, mengeluarkan orang dari room LiveKit |
| `push` | Web Push: memvalidasi subscription (hanya layanan push browser), mengenkripsi payload (RFC 8291 `aes128gcm`), menandatangani request dengan VAPID (RFC 8292, ES256). Rust murni di atas RustCrypto, tanpa OpenSSL |
| `security` | Slug acak (CSPRNG, rejection sampling), token 256-bit, hash SHA-256, kredensial TURN HMAC |

### Model sinkronisasi

Server tidak menjalankan jam per room. State playback berisi posisi pada saat perubahan terakhir:

```
posisi_sekarang = position + (status == playing ? (server_now - updated_at) / 1000 : 0)
```

- Setiap perubahan menaikkan `version`; klien mengabaikan update yang lebih tua.
- Klien mengestimasi offset jam server dengan ping/pong (gaya NTP, sampel RTT terendah).
- Setiap 500 ms player lokal dibandingkan dengan posisi seharusnya. Jika selisih > 1 detik saat play (0,5 detik saat pause), player di-seek ke posisi room, dengan jeda minimal 2,5 detik antarkoreksi.
- **Pencegahan loop**: perubahan yang dilakukan oleh kode membuka jendela "suppress" singkat. Event player hanya dianggap niat pengguna jika iframe YouTube sedang fokus (pengguna mengklik di dalamnya). Seek pengguna dideteksi dari *lompatan* waktu lokal antar-sampel, bukan dari selisih terhadap target, sehingga buffering tidak salah dianggap seek.
- Peserta tanpa izin yang menekan play/pause/seek di player YouTube langsung dikembalikan ke state room dan diberi penjelasan.
- Kecepatan putar dikunci 1× karena tidak dibagikan antar-penonton.
- Video selesai: laporan `playback_ended` diterima dari pengontrol, atau dari penonton jika estimasi server juga menunjukkan video hampir habis. Dengan begitu penonton tidak bisa melewati video.

### Vote skip

- Siapa pun di room bisa memberi vote untuk melewati video yang sedang diputar (`skip_vote`), dan menariknya kembali.
- Video dilewati saat jumlah vote mencapai mayoritas peserta yang sedang tersambung: `floor(n / 2) + 1`. Hanya vote dari peserta yang tersambung yang dihitung, dan orang yang keluar dari room dihapus dari daftar.
- Vote berlaku untuk satu video; vote direset saat video berganti.
- Vote yang berhasil diumumkan dengan `skip_votes` (`skipped: true`), diikuti `queue_updated`/`playback_changed` biasa dengan `actor_id: null`.
- Host (dan semua orang, jika `control_mode = everyone`) tetap bisa melewati video langsung dengan `queue_skip`.

### Mulai bareng (hitung mundur)

"Mulai bareng" membuat satu grup menekan play di saat yang sama, dengan hitungan 3-2-1 yang dilihat semua orang.

- `start_countdown` datang dari orang yang boleh mengontrol playback (kena rate limit seperti event playback lain), dan hanya saat video sedang diputar atau di-pause; selain itu ditolak dengan `conflict`.
- Server mem-pause di posisi saat ini (sehingga `version` naik) lalu mem-broadcast `playback_changed`, kemudian `countdown` dengan `start_at = now + 3000` (`COUNTDOWN_MS`), `version` saat di-pause, dan `actor_id`.
- Sebuah task timer di luar lock room bangun setelah 3 detik dan memutar video dari posisi yang sama, dengan `actor_id: null`, tetapi hanya jika `playback.version` masih sama dengan versi hitung mundur dan video masih di-pause. Perubahan apa pun di antaranya (pause, seek, video lain) menaikkan versi sehingga hitung mundur batal tanpa pemberitahuan.
- Klien menggambar 3-2-1 besar di atas video berdasarkan `start_at` dan offset jam server-nya, menampilkan "Mulai!" sebentar, lalu menghapus overlay begitu versi playback yang lebih baru datang.

### Presence & reconnect

- `POST /join` **memesan kursi** (60 detik), lalu WebSocket mengautentikasi dengan frame pertama `auth`. Token tidak pernah ada di URL, sehingga tidak tercatat di log proxy.
- Koneksi terputus → peserta ditandai `connected: false` selama **grace window** (default 45 detik, cukup untuk ponsel yang berpindah aplikasi atau jaringan). Reconnect dalam jendela itu memakai identitas yang sama tanpa event keluar.
- Satu peserta bisa punya beberapa koneksi; presence dihitung per peserta.
- Jika server melupakan sesi (restart), WebSocket ditutup dengan kode `4401` dan klien otomatis bergabung ulang dengan nama yang dipakai tab itu.
- Host: pembuat room (pemegang host key) selalu merebut kembali peran host saat masuk.
- **Pengalihan host**: jika host terputus selama 10 detik (`HOST_HANDOFF_MS`), kontrol host berpindah ke pembuat room jika ia tersambung, jika tidak ke anggota tersambung yang paling lama berada di room. Host semula mendapatkannya kembali saat reconnect, kecuali host sengaja dipindahkan (`host_transfer`) di antara waktu itu. Dengan begitu room tidak pernah macet tanpa ada yang bisa menekan play.
- Jika host keluar, penggantinya dipilih dengan cara yang sama (pembuat room dulu, lalu anggota terlama), dengan mengutamakan anggota yang tersambung.

### Konkurensi & performa

- Satu `std::sync::Mutex` per room, hanya dipegang untuk kerja sinkron (transisi state + `try_send`). Tidak ada `await` saat lock dipegang.
- Event broadcast diserialisasi **sekali** lalu dikirim ke channel berbatas (256) milik setiap koneksi. Konsumen yang terlalu lambat diputus, bukan dibiarkan menumpuk memori.
- Lookup oEmbed dan playlist (jaringan) dilakukan di luar lock; izin dicek sebelum dan sesudahnya.
- Batas: body HTTP 16 KB, pesan WebSocket 64 KB, sinyal WebRTC 24 KB, chat 500 karakter, riwayat chat di memori 100 pesan, antrean 100, impor playlist 50 video, subscription push 200 per room.
- Notifikasi push dikirim dari task latar belakang, di luar lock room, sehingga layanan push yang lambat tidak pernah menahan room.

### Data persisten (PostgreSQL)

| Tabel | Isi | Alasan |
| --- | --- | --- |
| `rooms` | slug, nama, hash host key, pengaturan (termasuk `permanent`), pemilik API key, `last_active_at`, `saved_state` | Link undangan harus tetap valid setelah restart; video dan antrean tetap ada setelahnya |
| `chat_messages` | riwayat chat per room, termasuk kutipan `reply_to` | Peserta yang terlambat bisa mengikuti percakapan |
| `api_keys` | label, **hash** key, waktu pakai | Autentikasi API developer |
| `server_settings` | pasangan key/value kecil untuk seluruh server; saat ini private key VAPID (`vapid_private_key`) | Setiap subscription push terikat pada key VAPID server, jadi key itu harus bertahan setelah restart |
| `push_subscriptions` | per room: endpoint push, `p256dh`, `auth`, bahasa (`en`/`id`); unik per room + endpoint | Notifikasi "ada yang mulai nonton" untuk browser yang memintanya |

Tidak ada email, password, atau IP yang disimpan. Subscription push mengidentifikasi sebuah browser di layanan push-nya, bukan seseorang. Room yang tidak dikunjungi selama `ROOM_RETENTION_DAYS` (default 30) dihapus bersama chat-nya, kecuali room **permanen** (pengaturan host "Keep this room forever"), yang tidak pernah dihapus.

#### State room yang disimpan

Video yang sedang diputar (playback) dan antrean disimpan sebagai JSONB di `rooms.saved_state`:

- setiap 5 detik, jika berubah sejak penyimpanan terakhir;
- sebelum room yang menganggur dilepas dari memori;
- saat server dimatikan dengan rapi (SIGTERM / Ctrl+C).

Saat room dimuat lagi: jika video sedang diputar dan server kembali dalam 2 menit (`RESUME_WINDOW_MS`), video tetap berjalan seolah jam terus berjalan (player penonton juga tetap berjalan selama itu). Jika lebih lama, video di-pause di posisi saat disimpan.

Sesi tidak disimpan; sesi hanya ada di memori. Setelah restart klien bergabung ulang tanpa terlihat (dengan participant id baru), dan pembuat room mendapatkan kembali peran host lewat host key yang tersimpan.

Migrasi `20261028000003_room_state.sql` menambahkan `rooms.saved_state JSONB`, `rooms.permanent BOOLEAN`, dan `chat_messages.reply_to JSONB`. Migrasi `20261028000004_push.sql` menambahkan `server_settings` dan `push_subscriptions` (ikut terhapus bersama room-nya).

### Notifikasi push

Sebuah browser bisa minta diberi tahu saat ada yang mulai nonton di sebuah room, bahkan saat Partello tertutup (tombol lonceng di header room). Ini Web Push standar, diimplementasikan di `push.rs` tanpa library push pihak ketiga.

- **Kapan**: seseorang tersambung ke room yang sebelumnya tidak ada orang tersambung. Setiap subscription room itu lalu menerima notifikasi berjudul nama room: "Riky sedang nonton Wildwood. Ketuk untuk gabung." (judul video sampai tanda `|` pertama), atau "Riky mulai nonton bareng. Ketuk untuk gabung." jika belum ada video. Teksnya dalam bahasa yang dipilih saat subscribe (`en` atau `id`); mengetuknya membuka `/r/{slug}`.
- **Seberapa sering**: paling banyak satu per room setiap 10 menit (dicatat bersama room selama room ada di memori), dan tidak ada sama sekali dalam 2 menit pertama setelah server menyala, karena saat itu semua orang sedang reconnect dan room tidak benar-benar "dimulai".
- **Enkripsi** (RFC 8291): satu record `aes128gcm` per pesan, dengan key P-256 sementara yang baru dan salt acak setiap kali; key `p256dh` dan `auth` milik browser menurunkan content key. **Tanda tangan** (RFC 8292): JWT VAPID ES256 dengan `aud` = origin layanan push, `exp` 12 jam, dan `sub` = `VAPID_SUBJECT`. Request membawa `TTL` 6 jam, `Urgency: normal`, dan `Topic` per room sehingga notifikasi yang belum terkirim diganti oleh yang lebih baru.
- **Key VAPID**: dibuat saat pertama kali menyala dan disimpan di `server_settings`, atau dipatok dengan `VAPID_PRIVATE_KEY`. Browser subscribe dengan public key-nya (`GET /api/push/key`), jadi mengganti key diam-diam mengakhiri semua subscription yang ada. Dengan store in-memory (tanpa `DATABASE_URL`) key baru dibuat setiap kali menyala. Jika key gagal dimuat, push dimatikan dengan peringatan dan endpoint push menjawab `unavailable`.
- **Tanpa SSRF**: subscription hanya diterima jika endpoint-nya `https` di port 443 pada layanan push browser yang dikenal (`fcm.googleapis.com`, `android.googleapis.com`, `push.services.mozilla.com`, `push.apple.com`, `notify.windows.com`, `push.api.chrome.google.com`, atau subdomain-nya), maksimal 1024 karakter, dengan `p256dh` P-256 yang valid dan `auth` 16 byte. Dengan begitu server tidak pernah bisa disuruh memanggil alamat sembarang. Endpoint adalah URL kapabilitas dan tidak pernah dicatat di log.
- **Pembersihan**: layanan push yang menjawab 404 atau 410 berarti subscription sudah tidak ada (berhenti langganan, kedaluwarsa), dan subscription itu dihapus. Satu room menyimpan maksimal 200 subscription; lebih dari itu, yang paling lama dibuang. Subscribe lagi dari browser yang sama memperbarui key dan bahasanya.
- **Service worker**: menampilkan notifikasi kecuali ada jendela yang terlihat dan sudah membuka room itu; ketukan memfokuskan jendela yang sudah membuka room tersebut atau membuka jendela baru.

## Frontend

- `RoomController` adalah satu-satunya yang menyentuh WebSocket; komponen memakai aksi tingkat niat (`control`, `queueAdd`, `sendChat`, …).
- `room/store.ts` berisi reducer murni plus external store kecil. Komponen berlangganan potongan state lewat `useSyncExternalStore`, sehingga pesan chat tidak me-render ulang player.
- `SyncedPlayer` memakai YouTube IFrame Player API resmi dengan kontrol bawaan YouTube (tanpa overlay di atas iframe, sesuai kebijakan YouTube). Kontrol Partello berada di bawah player.
- Halaman room dan developer di-lazy-load; landing tidak memuat kode WebRTC/player. Klien LiveKit hanya dimuat dalam mode SFU.
- **Reaksi**: bar reaksi di bawah player; emoji melayang di atas video untuk semua orang. Di bawah `prefers-reduced-motion` emoji hanya memudar. Tombol pin di sebelahnya mengirim "📌 m:ss" untuk momen saat ini ke chat.
- **Mulai bareng**: tombol timer di bar playback (untuk orang yang boleh mengontrol playback) mengirim `start_countdown`; `CountdownOverlay` menampilkan 3-2-1 besar di atas video.
- **Mode bioskop**: tombol di bar playback membuat area player layar penuh dengan Fullscreen API, atau tata letak tetap seukuran jendela jika API itu tidak ada (iPhone). `TheaterOverlay` menaruh pesan chat terbaru, kotak chat ringkas, reaksi, dan kontrol call di atas video; chat bisa disembunyikan. Esc atau tombol yang sama untuk keluar.
- **Chat**: tombol balas per pesan dengan blok kutipan; autocomplete `@mention` (ketik `@`, lalu panah/Tab/Enter); mention untuk Anda disorot. Suara notifikasi dibuat dengan Web Audio: mention dan balasan untuk Anda selalu berbunyi, pesan lain hanya jika chat tidak terlihat. Tombol lonceng mematikannya; pilihan disimpan per perangkat. Mention atau balasan saat tab tersembunyi juga memunculkan notifikasi sistem, jika izin notifikasi sudah diberikan sebelumnya (misalnya lewat lonceng notifikasi room).
- **Timestamp di chat**: waktu seperti `12:45` atau `1:02:03` di dalam pesan menjadi link. Bagi orang yang boleh mengontrol playback, klik akan memindahkan seluruh room ke detik itu (seek `playback_control` biasa); orang lain melihatnya disorot. Waktu yang melewati akhir video tetap teks biasa. Semuanya di sisi klien (`features/chat/timestamps.ts`), tanpa perubahan protokol.
- **Antrean**: jika link yang ditempel berisi playlist, tombol playlist mengimpornya (hanya jika server punya `YOUTUBE_API_KEY`).
- **Pengaturan**: dialog pengaturan host berisi "Keep this room forever" (`permanent`).
- **i18n** (`web/src/i18n`): teks ditulis dalam bahasa Inggris di kode (`t('Join call')`) dan dicari di kamus `id.ts` saat bahasa Indonesia dipilih. Pesan error dari server juga diterjemahkan berdasarkan teksnya. Test `i18n.test.ts` memindai seluruh kode dan gagal bila ada teks tanpa terjemahan atau placeholder yang tidak cocok.
- **PWA**: web manifest dengan ikon dan service worker (`web/public/sw.js`, hanya didaftarkan di build produksi) membuat Partello bisa dipasang. Halaman memakai network-first dengan app shell yang di-cache sebagai cadangan saat offline; `/assets/` dan `/icons/` memakai cache-first (file ber-hash tidak pernah berubah); `/api/` dan health check tidak pernah di-cache atau dicegat. Landing page menampilkan tombol "Pasang aplikasi" saat browser menawarkan pemasangan (`beforeinstallprompt`).
- **Subscription push** (`lib/pwa.ts`, `NotifyButton`): lonceng di header room meminta izin, men-subscribe browser ini dengan key VAPID server, dan mendaftarkannya untuk room itu dalam bahasa antarmuka. Di iPhone dan iPad tombol ini lebih dulu menjelaskan bahwa Partello harus ditambahkan ke Layar Utama (iOS 16.4+).
- Landing page mengikuti arah desain dari Taste Skill ("page is the object": invite link sebagai tiket bioskop). Motion memakai transform/opacity/clip-path dan dimatikan di bawah `prefers-reduced-motion`.

## Voice & video (WebRTC)

Call berjalan dalam salah satu dari dua mode, yang dikirim ke klien sebagai `limits.call_mode`. Keduanya mengimplementasikan interface `CallTransport` yang sama: `MeshCall` (`web/src/features/call/mesh.ts`) dan `SfuCall` (`web/src/features/call/sfu.ts`). Bagian UI call lainnya tidak perlu tahu mode mana yang dipakai.

### Mesh (default)

- Setiap anggota call memegang satu `RTCPeerConnection` per anggota lain. Server hanya meneruskan signaling (`voice_signal`) antar-anggota call.
- Tepat satu sisi setiap pasangan yang membuat offer (id lebih kecil). Sisi itu membuat transceiver audio + video sejak awal, sehingga **kamera on/off cukup `replaceTrack`** tanpa renegosiasi. Pola *perfect negotiation* tetap dipakai untuk ICE restart.
- Bitrate video dibatasi menurut jumlah peer (900/500/300 kbps); capture 640×360 @ 24 fps.
- **Batas mesh**: upload tumbuh linear dengan jumlah peserta (8 orang dengan kamera = 7 stream keluar per orang). Karena itu `ROOM_MAX_VOICE` default-nya 8 di mode ini. Untuk call yang lebih besar, pakai SFU.

### SFU (LiveKit, opsional)

Diaktifkan dengan mengisi `LIVEKIT_URL`, `LIVEKIT_API_KEY`, dan `LIVEKIT_API_SECRET` (plus opsional `LIVEKIT_API_URL`, default `LIVEKIT_URL` dengan `wss://` → `https://`). Setelah itu `call_mode` menjadi `sfu` dan `ROOM_MAX_VOICE` default-nya sama dengan `ROOM_MAX_PARTICIPANTS`, bukan 8. Setiap peserta mengirim audio dan videonya sekali saja, ke LiveKit, yang meneruskannya ke peserta lain.

Partello tetap yang menentukan siapa yang ada di call:

- Server mengeluarkan token LiveKit hanya setelah menerima `voice_join`, dan hanya mengirimnya ke peserta itu (`call_token`). Token berupa JWT HS256: `iss` = API key, `sub` = participant id, `name` = nama tampilan, dan video grant untuk room `partello-<uuid room dalam hex>` dengan `roomJoin`, `canPublish`, `canSubscribe`, `canPublishData: false`, `canPublishSources: ["camera", "microphone"]`. Token kedaluwarsa setelah 10 menit (hanya perlu dipakai untuk tersambung). API secret tidak pernah keluar dari server.
- Nama room LiveKit berasal dari id internal room yang tidak pernah ditampilkan, sehingga mengetahui link undangan tidak cukup untuk menemukan call-nya.
- Saat seseorang meninggalkan room Partello karena alasan apa pun (keluar, grace window habis, dikeluarkan host) atau mengirim `voice_leave`, server mengeluarkannya dari room LiveKit lewat API Twirp `livekit.RoomService/RemoveParticipant`.
- Frontend memuat `livekit-client` sesuai kebutuhan, hanya dalam mode SFU. `adaptiveStream` dimatikan karena video ditampilkan di elemen milik Partello sendiri, `dynacast` dinyalakan, dan kamera dikirim dengan simulcast.
- Mute dan push-to-talk bekerja sama seperti di mesh: track mikrofon dinonaktifkan.
- CSP `connect-src` menyertakan origin LiveKit (`wss://` dan `https://`).

### Berlaku untuk kedua mode

- Izin mikrofon diminta hanya saat menekan "Join call"; izin kamera hanya saat menyalakan kamera.
- Indikator bicara dihitung lokal dari level audio (WebAudio), tanpa trafik tambahan.
- **Kebocoran suara video ke mikrofon.** Echo cancellation browser hanya menghapus audio call, bukan audio iframe YouTube yang keluar dari speaker yang sama. Mitigasi berlapis: (1) constraint `echoCancellation: 'all'` (pembatalan semua audio sistem di Chromium baru; browser lama membacanya sebagai `true`) dan `voiceIsolation`; (2) **auto-ducking**: volume video turun ke 65% selama call dan ke 20% saat ada yang bicara, lalu dikembalikan; (3) **push-to-talk** (tahan tombol/Spasi), mikrofon hanya aktif saat ditahan; (4) anjuran headset di UI.
- Semua track, peer connection, dan listener dibersihkan saat keluar call, meninggalkan room, atau unmount.

### TURN

STUN saja sudah cukup untuk sebagian besar jaringan rumah. Ponsel dengan data seluler (CGNAT) dan jaringan kantor yang ketat butuh relay TURN. Server memakai sumber pertama yang dikonfigurasi:

1. **Cloudflare Realtime TURN** (`CLOUDFLARE_TURN_KEY_ID` + `CLOUDFLARE_TURN_API_TOKEN`): server mengirim POST ke endpoint `generate-ice-servers` milik Cloudflare dengan TTL 6 jam, menyimpan hasilnya di cache selama 3 jam, dan membuang URL port 53. Kredensialnya dipakai bersama oleh semua peserta. Jika request gagal, klien hanya mendapat STUN dan server mencatat peringatan.
2. **Shared secret coturn** (`TURN_URLS` + `TURN_SHARED_SECRET`, gaya coturn `use-auth-secret`): server mencetak kredensial berumur pendek per peserta. Secret tidak pernah dikirim ke browser.
3. **Kredensial statis** (`TURN_URLS` + `TURN_USERNAME` + `TURN_CREDENTIAL`, mis. akun gratis Metered.ca). Kredensial ini terlihat oleh anggota room.

## Skala dan state terdistribusi

Konsekuensi state in-memory:

1. **Restart server** menghapus presence, sesi, dan status call. Room, chat, video yang sedang diputar, dan antrean tetap ada di PostgreSQL (video dan antrean disimpan setiap 5 detik dan saat shutdown, lihat [State room yang disimpan](#state-room-yang-disimpan)); klien bergabung ulang otomatis.
2. **Multi-instance** butuh semua koneksi satu room berada di instance yang sama. Opsi termudah: sticky routing berdasarkan slug di load balancer.

Jalur migrasi bila dibutuhkan:

- `room::state` sudah murni dan bisa diserialisasi. State room bisa dipindah ke Redis (atau dipegang oleh satu "pemilik room" per room) tanpa mengubah aturan.
- `Hub::dispatch` adalah satu-satunya tempat event dikirim keluar. Fan-out lintas instance cukup ditambahkan di sana (mis. Redis pub/sub per room).
- Sesi bisa dipindah ke tabel PostgreSQL atau Redis.
- Rate limit per IP/key perlu penyimpanan bersama (Redis) atau dipindah ke edge/proxy.
