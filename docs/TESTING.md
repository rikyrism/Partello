# Panduan Testing

## Test otomatis

### Backend (`server/`)

```bash
cargo test                                   # unit + integration (store in-memory)
TEST_DATABASE_URL=postgres://partello:partello@localhost:5433/partello cargo test --test postgres
```

| Lingkup | Lokasi | Yang diuji |
| --- | --- | --- |
| State machine room | `src/room/state_tests.rs` | Host pertama, kapasitas, reservasi tak terlihat, creator merebut host, grace window reconnect, multi-tab, izin playback host-only, posisi tidak valid, antrean & izinnya, kapasitas antrean, laporan `ended` yang divalidasi, chat ring buffer, batas & signaling voice, transfer host |
| Playback | `src/room/playback.rs` | Transisi play/pause/seek/load/finish, clamp posisi, versi monoton, durasi |
| Antrean | `src/room/queue.rs` | Kapasitas, hapus, pop, pindah urutan |
| Validasi | `src/validation.rs`, `src/youtube.rs`, `src/youtube_search.rs` | Nama, chat, karakter bidi/kontrol, bentuk URL YouTube, parsing respons Data API |
| Protokol | `src/realtime/protocol.rs` | Event valid & payload rusak ditolak |
| Keamanan | `src/security.rs`, `src/rate_limit.rs` | Slug, hash, rumus kredensial TURN (dicocokkan dengan `openssl`), token bucket, `X-Forwarded-For` |
| **End-to-end** | `tests/realtime.rs` | Server nyata + 2–3 klien WebSocket: playback/antrean/chat tersinkron, peserta terlambat menerima state terkini, reconnect dalam grace, keluar setelah grace, leave langsung, promosi host, payload rusak, rate limit chat, relay signaling, auth palsu & origin asing, validasi REST & room penuh, izin pengaturan, API developer, unload room idle & purge retensi, health, search tanpa key |
| PostgreSQL | `tests/postgres.rs` | Migrasi, room, chat terbaru, API key, cascade delete |

### Frontend (`web/`)

```bash
npm test          # Vitest + Testing Library (jsdom)
npm run lint
npm run typecheck
```

| File | Yang diuji |
| --- | --- |
| `syncMath.test.ts` | Posisi seharusnya, deteksi lompatan vs buffering, keputusan koreksi drift, seek pengguna tanpa izin dikembalikan |
| `store.test.ts` | Snapshot, pesan sistem masuk/keluar/call/kamera, update playback usang diabaikan, rekonsiliasi chat optimistis |
| `parsing.test.ts` | Parser link YouTube, kode room, estimasi jam server, format waktu |
| `components.test.tsx` | Chat (Enter kirim, Shift+Enter baris baru, empty/reconnecting state), antrean (validasi link, pencarian YouTube via server, mode host-only), join gate (validasi nama, room penuh) |
| `i18n.test.ts` | Setiap `t()`/`translate()`/`msg()` di kode punya terjemahan Indonesia, nama genre & catatan sistem chat lengkap, placeholder `{…}` konsisten, fallback ke Inggris |

## Uji manual: dua browser, playback tersinkron

Sudah dijalankan pada pengembangan ini di Chrome dengan dua tab (host & tamu).

1. Jalankan server (`cargo run`) dan buka `http://localhost:8080`.
2. **Browser A**: tulis nama di tiket → *Create a room*. Room terbuka dengan status *Live*.
3. Klik *Invite* → link tersalin.
4. **Browser B** (jendela incognito atau browser lain, agar localStorage terpisah): buka link → isi nama → *Join the room*.
   - A melihat "B joined" di chat dan jumlah *People* naik.
   - B melihat label "Host controls playback".
5. A: tab *Queue* → tempel `https://www.youtube.com/watch?v=aqz-KE-bpKQ` → *Add*. Video diputar di kedua browser.
6. A: tombol pause di bawah player → kedua browser berhenti di detik yang sama.
7. B: klik video untuk play → ditolak, video kembali ke posisi room, dan muncul toast "Only the host can control playback".
8. A: geser slider seek ke tengah → B melompat ke posisi yang sama (selisih ≤ 1 detik).
9. B: kirim chat → muncul di A secara real-time.
10. B: reload halaman → kembali sebagai peserta yang sama, video lanjut di posisi room.
11. Matikan Wi-Fi B selama 5 detik lalu nyalakan → indikator "Reconnecting", lalu tersambung kembali tanpa pesan "left".

## Uji manual WebRTC (memerlukan browser & jaringan nyata)

Otomatisasi di repo menguji signaling, bukan media. Skenario berikut perlu dijalankan dengan perangkat nyata:

| # | Skenario | Yang diharapkan |
| --- | --- | --- |
| 1 | Dua laptop di Wi-Fi yang sama, *Join call* | Izin mikrofon diminta saat itu juga (bukan saat masuk room); suara terdengar dua arah; ring bicara menyala |
| 2 | Mute/unmute | Ikon mic merah di tile dan daftar People; pesan sistem "muted the microphone" |
| 3 | Kamera on/off saat call berjalan | Video muncul/hilang tanpa reconnect; avatar saat kamera mati |
| 4 | Tolak izin mikrofon | Toast menjelaskan cara mengizinkan; tetap bisa menonton dan chat |
| 5 | Kamera dipakai aplikasi lain (Zoom) | Toast "being used by another app"; call audio tetap jalan |
| 6 | Laptop ↔ ponsel via data seluler | Tanpa TURN bisa gagal tersambung (ikon loading pada tile). Dengan `TURN_URLS` + `TURN_SHARED_SECRET` harus tersambung |
| 7 | 4–8 peserta dengan kamera | Kualitas menurun bertahap (bitrate dibatasi); peserta ke-9 ditolak "call is full" |
| 8 | Keluar call / tutup tab | Lampu kamera/mic perangkat padam; peer lain melihat peserta keluar dari call |
| 9 | Headset vs speaker laptop | Tidak ada echo (echo cancellation aktif) |
| 10 | Volume per peserta (People → ikon volume) | Hanya suara orang tersebut yang berubah |
| 11 | Speaker laptop (tanpa headset) saat video diputar | Teman mendengar suara Anda jelas; suara video tertekan (voice isolation) dan video mengecil saat ada yang bicara (auto-ducking) |
| 12 | Push-to-talk (ikon tangan) | Teman hanya mendengar Anda selama tombol "Tahan untuk bicara" atau Spasi ditahan; mengetik di chat tidak memicu Spasi |

Untuk uji di jaringan berbeda tanpa deploy, jalankan Partello di balik tunnel HTTPS (mis. `cloudflared tunnel --url http://localhost:8080`) dan tambahkan origin tunnel ke `ALLOWED_ORIGINS`.

## Gate desain (Taste Skill)

```bash
node <taste-skill>/scripts/lint.mjs web/src/pages web/src/index.css
node <taste-skill>/scripts/inspect.mjs http://localhost:8080/
```

Hasil terakhir: lint 0 FAIL, inspect 0 FAIL. WARN yang tersisa dijelaskan di README.
