# Arsitektur Partello

## Gambaran umum

```
Browser (React)                                Server (Rust, satu proses)
┌──────────────────────────────┐   HTTPS     ┌───────────────────────────────────────┐
│ Landing / Room / Developers  │────────────▶│ routes/  REST + static SPA             │
│                              │             │   ├─ rooms.rs      (web app)           │
│ RoomController               │  WebSocket  │   └─ developer.rs  (API v1)            │
│  ├─ RoomSocket (auth, ping,  │◀───────────▶│ realtime/ws.rs   1 task per koneksi   │
│  │   reconnect, jam server)  │             │ realtime/hub.rs  room hidup + sesi    │
│  └─ store (reducer murni)    │             │ room/state.rs    aturan murni (no IO) │
│                              │             │ database.rs      PostgreSQL           │
│ SyncedPlayer ◀── YouTube IFrame Player API │ youtube*.rs      oEmbed + Data API    │
│ MeshCall ◀──── WebRTC P2P ───▶ peer lain   └───────────────────────────────────────┘
└──────────────────────────────┘
```

**Modular monolith**: satu binary Rust menyajikan REST, WebSocket, dan hasil build frontend. Tidak ada microservice dan tidak ada Redis. Keduanya baru masuk akal setelah ada kebutuhan multi-instance yang konkret.

## Backend

| Modul | Tanggung jawab |
| --- | --- |
| `config` | Membaca environment variable; ringkasan log tanpa secret |
| `room::state` | **Seluruh aturan room**: presence, host, izin, playback, antrean, chat, voice. Murni, tanpa IO. Setiap operasi mengembalikan daftar `Effect` (broadcast, kirim ke satu orang, simpan chat) |
| `room::playback` | State playback otoritatif: `position` pada `updated_at` + `status` + `version` |
| `room::queue` | Antrean "up next" |
| `realtime::hub` | Memiliki room yang sedang hidup dan sesi; menerjemahkan `Effect` ke frame WebSocket dan penulisan database |
| `realtime::ws` | Satu task per koneksi: auth frame pertama, rate limit per jenis event, heartbeat, idle timeout |
| `realtime::protocol` | Enum event klien/server bertipe (serde, `type` sebagai tag) |
| `routes` | Handler HTTP tipis: DTO request/response eksplisit, logika ada di hub/state |
| `database` | `Store` = enum `Postgres | Memory`, sehingga test end-to-end berjalan tanpa database |
| `youtube` | Parser URL YouTube, oEmbed untuk judul/thumbnail sekaligus cek embeddability |
| `youtube_search` | YouTube Data API v3 (opsional), hanya di server, dengan cache |
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

### Presence & reconnect

- `POST /join` **memesan kursi** (60 detik), lalu WebSocket mengautentikasi dengan frame pertama `auth`. Token tidak pernah ada di URL, sehingga tidak tercatat di log proxy.
- Koneksi terputus → peserta ditandai `connected: false` selama **grace window** (default 45 detik, cukup untuk ponsel yang berpindah aplikasi atau jaringan). Reconnect dalam jendela itu memakai identitas yang sama tanpa event keluar.
- Satu peserta bisa punya beberapa koneksi; presence dihitung per peserta.
- Jika server melupakan sesi (restart), WebSocket ditutup dengan kode `4401` dan klien otomatis bergabung ulang dengan nama yang dipakai tab itu.
- Host: pembuat room (pemegang host key) selalu merebut kembali peran host saat masuk. Jika host keluar, peserta terlama menjadi host.

### Konkurensi & performa

- Satu `std::sync::Mutex` per room, hanya dipegang untuk kerja sinkron (transisi state + `try_send`). Tidak ada `await` saat lock dipegang.
- Event broadcast diserialisasi **sekali** lalu dikirim ke channel berbatas (256) milik setiap koneksi. Konsumen yang terlalu lambat diputus, bukan dibiarkan menumpuk memori.
- Lookup oEmbed (jaringan) dilakukan di luar lock; izin dicek sebelum dan sesudahnya.
- Batas: body HTTP 16 KB, pesan WebSocket 64 KB, sinyal WebRTC 24 KB, chat 500 karakter, antrean 100.

### Data persisten (PostgreSQL)

| Tabel | Isi | Alasan |
| --- | --- | --- |
| `rooms` | slug, nama, hash host key, pengaturan, pemilik API key, `last_active_at` | Link undangan harus tetap valid setelah restart |
| `chat_messages` | riwayat chat per room | Peserta yang terlambat bisa mengikuti percakapan |
| `api_keys` | label, **hash** key, waktu pakai | Autentikasi API developer |

Tidak ada email, password, atau IP yang disimpan. Room yang tidak dikunjungi selama `ROOM_RETENTION_DAYS` (default 30) dihapus bersama chat-nya.

## Frontend

- `RoomController` adalah satu-satunya yang menyentuh WebSocket; komponen memakai aksi tingkat niat (`control`, `queueAdd`, `sendChat`, …).
- `room/store.ts` berisi reducer murni plus external store kecil. Komponen berlangganan potongan state lewat `useSyncExternalStore`, sehingga pesan chat tidak me-render ulang player.
- `SyncedPlayer` memakai YouTube IFrame Player API resmi dengan kontrol bawaan YouTube (tanpa overlay di atas iframe, sesuai kebijakan YouTube). Kontrol Partello berada di bawah player.
- Halaman room dan developer di-lazy-load; landing tidak memuat kode WebRTC/player.
- **i18n** (`web/src/i18n`): teks ditulis dalam bahasa Inggris di kode (`t('Join call')`) dan dicari di kamus `id.ts` saat bahasa Indonesia dipilih. Pesan error dari server juga diterjemahkan berdasarkan teksnya. Test `i18n.test.ts` memindai seluruh kode dan gagal bila ada teks tanpa terjemahan atau placeholder yang tidak cocok.
- Landing page mengikuti arah desain dari Taste Skill ("page is the object": invite link sebagai tiket bioskop). Motion memakai transform/opacity/clip-path dan dimatikan di bawah `prefers-reduced-motion`.

## Voice & video (WebRTC)

- **Mesh**: setiap anggota call memegang satu `RTCPeerConnection` per anggota lain. Server hanya meneruskan signaling (`voice_signal`) antar-anggota call.
- Tepat satu sisi setiap pasangan yang membuat offer (id lebih kecil). Sisi itu membuat transceiver audio + video sejak awal, sehingga **kamera on/off cukup `replaceTrack`** tanpa renegosiasi. Pola *perfect negotiation* tetap dipakai untuk ICE restart.
- Izin mikrofon diminta hanya saat menekan "Join call"; izin kamera hanya saat menyalakan kamera.
- Bitrate video dibatasi menurut jumlah peer (900/500/300 kbps); capture 640×360 @ 24 fps.
- Indikator bicara dihitung lokal dari level audio (WebAudio), tanpa trafik tambahan.
- **Kebocoran suara video ke mikrofon.** Echo cancellation browser hanya menghapus audio call, bukan audio iframe YouTube yang keluar dari speaker yang sama. Mitigasi berlapis: (1) constraint `echoCancellation: 'all'` (pembatalan semua audio sistem di Chromium baru; browser lama membacanya sebagai `true`) dan `voiceIsolation`; (2) **auto-ducking**: volume video turun ke 65% selama call dan ke 20% saat ada yang bicara, lalu dikembalikan; (3) **push-to-talk** (tahan tombol/Spasi), mikrofon hanya aktif saat ditahan; (4) anjuran headset di UI.
- Semua track, peer connection, dan listener dibersihkan saat keluar call, meninggalkan room, atau unmount.
- **TURN**: server mencetak kredensial berumur pendek (`TURN_SHARED_SECRET`, gaya coturn `use-auth-secret`). Secret tidak pernah dikirim ke browser.
- **Batas mesh**: upload tumbuh linear dengan jumlah peserta (8 orang dengan kamera = 7 stream keluar per orang). Untuk room besar, ganti implementasi `CallTransport` (`web/src/features/call/mesh.ts`) dengan klien SFU seperti LiveKit atau mediasoup; signaling Partello tetap dipakai untuk presence.

## Skala dan state terdistribusi

Konsekuensi state in-memory:

1. **Restart server** menghapus presence, playback, antrean, dan status call. Room dan chat tetap ada di PostgreSQL; klien bergabung ulang otomatis.
2. **Multi-instance** butuh semua koneksi satu room berada di instance yang sama. Opsi termudah: sticky routing berdasarkan slug di load balancer.

Jalur migrasi bila dibutuhkan:

- `room::state` sudah murni dan bisa diserialisasi. State room bisa dipindah ke Redis (atau dipegang oleh satu "pemilik room" per room) tanpa mengubah aturan.
- `Hub::dispatch` adalah satu-satunya tempat event dikirim keluar. Fan-out lintas instance cukup ditambahkan di sana (mis. Redis pub/sub per room).
- Sesi bisa dipindah ke tabel PostgreSQL atau Redis.
- Rate limit per IP/key perlu penyimpanan bersama (Redis) atau dipindah ke edge/proxy.
