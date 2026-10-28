# Panduan Testing

[English](../TESTING.md) · Bahasa Indonesia

## Test otomatis

### Backend (`server/`)

```bash
cargo test                                   # semua: 70 unit + 32 end-to-end (store in-memory)
cargo test --lib                             # unit test saja (70)
TEST_DATABASE_URL=postgres://partello:partello@localhost:5433/partello cargo test --test postgres   # 1 test store PostgreSQL
cargo test --lib push::tests::vector -- --ignored --nocapture   # helper yang di-ignore: mencetak test vector Web Push
cargo test --lib push::live -- --ignored --nocapture            # helper yang di-ignore: request langsung ke Google FCM (butuh internet)
```

| Lingkup | Lokasi | Yang diuji |
| --- | --- | --- |
| State machine room | `src/room/state_tests.rs` | Host pertama, kapasitas, reservasi tak terlihat, creator merebut host, grace window reconnect, multi-tab, izin playback host-only, posisi tidak valid, antrean & izinnya, kapasitas antrean, laporan `ended` yang divalidasi, chat ring buffer, batas & signaling voice, transfer host, kick, reaksi terbatas pada set yang diizinkan, vote lewati (mayoritas melewati video, orang yang keluar tidak dihitung lagi), balasan mengutip pesan asli, kontrol host pindah ke orang yang masih tersambung saat host pergi dan kembali saat host datang lagi (penyerahan yang disengaja tidak dibatalkan), simpan & pulihkan video + antrean, playlist mengisi antrean sampai kapasitas, room permanen sebagai pengaturan host, hitung mundur mem-pause lalu memutar dari posisi yang sama, perubahan selama hitung mundur membatalkannya |
| Playback | `src/room/playback.rs` | Transisi play/pause/seek/load/finish, clamp posisi, versi monoton, durasi |
| Antrean | `src/room/queue.rs` | Kapasitas, hapus, pop, pindah urutan |
| Validasi | `src/validation.rs`, `src/youtube.rs`, `src/youtube_search.rs` | Nama, chat, karakter bidi/kontrol, bentuk URL YouTube, link playlist (mix YouTube ditolak), parsing respons Data API untuk pencarian dan playlist (urutan terjaga, video yang tidak bisa diputar dibuang), impor playlist nonaktif tanpa key |
| Protokol | `src/realtime/protocol.rs` | Event valid & payload rusak ditolak |
| Keamanan | `src/security.rs`, `src/rate_limit.rs` | Slug, hash, rumus kredensial TURN (dicocokkan dengan `openssl`), token bucket, `X-Forwarded-For` |
| ICE / TURN | `src/ice.rs` | Parsing respons Cloudflare TURN (entri STUN dan port 53 dibuang), kredensial tetap diteruskan apa adanya, secret coturn menang atas kredensial tetap |
| SFU | `src/sfu.rs` | Token join LiveKit terbatas pada satu call (room, `roomJoin`, hanya kamera + mikrofon, tanpa admin, kedaluwarsa singkat), tanda tangan HS256 |
| Web Push | `src/push.rs`, `src/realtime/hub.rs` | Payload bisa didekripsi oleh penerima RFC 8291 independen yang ditulis dari spesifikasi, salt dan key pengirim baru di setiap pesan, subscription terbatas pada layanan push browser (http, host tiruan, alamat IP, port lain, dan key rusak ditolak), JWT VAPID ditandatangani untuk origin layanan push dan bisa diverifikasi dengan public key, key yang tersimpan memulihkan identitas yang sama, `Topic` aman untuk URL dan pendek, teks notifikasi dalam bahasa pelanggan |
| **End-to-end** | `tests/realtime.rs` | Server nyata + 2–3 klien WebSocket: playback/antrean/chat tersinkron, peserta terlambat menerima state terkini, reconnect dalam grace, keluar setelah grace, leave langsung, promosi host, payload rusak, rate limit chat, relay signaling, auth palsu & origin asing, validasi REST & room penuh, izin pengaturan, API developer, unload room idle & purge retensi, health, search tanpa key, room ber-PIN, kick, reaksi dan vote lewati sampai ke semua orang, balasan chat membawa kutipan, video dan antrean bertahan saat restart (dua instance server berbagi satu store memori), impor playlist tanpa key, token SFU hanya untuk anggota call, CSP mencantumkan origin SFU, room permanen, hitung mundur (tamu ditolak di room host-only, pause → `countdown` → play untuk semua orang), subscription push (endpoint key 503 sebelum push aktif, butuh sesi, hanya layanan push browser, berhenti langganan) |
| PostgreSQL | `tests/postgres.rs` | Migrasi, room, chat terbaru beserta balasan, state room tersimpan, API key, cascade delete, room permanen lolos dari purge retensi, pengaturan server menyimpan nilai pertama yang ditulis, subscription push (subscribe ulang memperbarui di tempat, penghapusan) |

### Frontend (`web/`)

```bash
npm test          # Vitest + Testing Library (jsdom), 59 test
npm run lint
npm run typecheck
```

| File | Yang diuji |
| --- | --- |
| `syncMath.test.ts` | Posisi seharusnya, deteksi lompatan vs buffering, keputusan koreksi drift, seek pengguna tanpa izin dikembalikan |
| `store.test.ts` | Snapshot, pesan sistem masuk/keluar/call/kamera, update playback usang diabaikan, rekonsiliasi chat optimistis |
| `parsing.test.ts` | Parser link YouTube, kode room, estimasi jam server, format waktu |
| `features.test.ts` | @mention (tidak peka huruf besar/kecil, batas kata, nama terpanjang lebih dulu, nama yang sedang diketik), deteksi link playlist (mix dikecualikan), vote lewati (mayoritas orang yang hadir, vote untuk video saat ini, direset saat video berganti) |
| `invite.test.ts` | Pesan undangan dalam bahasa Inggris dan Indonesia, dengan PIN atau keterangan kepada siapa harus bertanya |
| `components.test.tsx` | Chat (Enter kirim, Shift+Enter baris baru, empty/reconnecting state), antrean (validasi link, pencarian YouTube via server, mode host-only), join gate (validasi nama, PIN, room penuh) |
| `timestamps.test.ts` | Timestamp di chat (`m:ss` dan `h:mm:ss` dikenali, angka, waktu, dan versi lain dibiarkan), hitung mundur di store (dimulai pada versi saat pause, berakhir dengan perubahan playback berikutnya, diabaikan jika playback sudah berubah) |
| `i18n.test.ts` | Setiap `t()`/`translate()`/`msg()` di kode punya terjemahan Indonesia, nama genre & catatan sistem chat lengkap, placeholder `{…}` konsisten, fallback ke Inggris |

### Enkripsi Web Push terhadap implementasi lain

Unit test mendekripsi dengan penerima yang ditulis di repo ini, jadi enkripsinya juga dicek dengan kode dari luar:

- **`http_ece`** (implementasi Node independen untuk enkripsi yang sama, dipakai paket `web-push`): vector yang dicetak `push::tests::vector` (private key browser, `p256dh`, `auth`, body terenkripsi) terdekripsi menjadi `hello from partello`.
- **Google FCM**: `push::live` mengirim request terenkripsi yang ditandatangani VAPID ke subscription FCM karangan. FCM menjawab **410** (subscription tidak ada): request sudah lolos autentikasi VAPID.

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

## Uji manual: fitur room

Pakai dua atau tiga perangkat (atau browser dengan localStorage terpisah) di room yang sama.

| # | Skenario | Yang diharapkan |
| --- | --- | --- |
| 1 | Reaksi dari dua perangkat | Setiap reaksi muncul di kedua layar hampir bersamaan |
| 2 | Vote lewati dengan 3 orang (host + 2 tamu) | Satu vote belum cukup; saat mayoritas memilih, video dilewati untuk semua orang dan chat mencatat bahwa room sepakat melewatinya. Vote direset di video berikutnya |
| 3 | Balas + @mention saat tab lawan di background | Balasan menampilkan pesan yang dikutip; orang yang di-mention atau dibalas mendengar suara mention (dua nada) meskipun tab-nya tidak di depan (suara chat aktif, dan ia sudah pernah mengklik halaman sejak dibuka, karena browser memblokir suara sebelum itu) |
| 4 | Restart server saat video diputar (`Ctrl+C`, lalu jalankan lagi dalam 2 menit) | Semua orang tersambung ulang sendiri; video yang sama tetap diputar di posisi yang kira-kira benar dan antrean utuh. Setelah lebih dari 2 menit, video menunggu dalam keadaan pause di posisi yang sama |
| 5 | Host menutup tab | Setelah sekitar 10 detik, orang yang masih di room mendapat kontrol playback; saat host kembali, kontrol kembali ke host |
| 6 | Tempel link playlist YouTube (dengan `YOUTUBE_API_KEY` terisi) | Muncul tawaran menambahkan seluruh playlist; video masuk ke antrean sampai batasnya. Mix (`list=RD…`) ditolak dengan penjelasan |
| 7 | Host mencentang "Simpan room ini selamanya" | Pengaturan tetap ada setelah reload; room tidak dihapus oleh `ROOM_RETENTION_DAYS` |
| 8 | Host menekan tombol timer ("Mulai bareng") saat dua perangkat menonton | Keduanya pause, 3-2-1 besar muncul di atas video di keduanya, lalu video diputar di keduanya pada saat yang sama dari posisi yang sama. Pause atau seek selama 3 detik itu membatalkannya. Tamu di room host-only tidak melihat tombolnya |
| 9 | Kirim "lihat 1:30" di chat, lalu klik waktunya; tekan tombol pin di sebelah reaksi | Bagi host waktunya berupa link dan room melompat ke 1:30 di semua perangkat; tamu tanpa kontrol hanya melihatnya disorot. Pin mengirim "📌 m:ss" untuk momen saat ini, yang bisa diklik dengan cara yang sama |
| 10 | Mode bioskop di browser desktop, lalu di ponsel (termasuk iPhone) | Player memenuhi layar (layar penuh sungguhan jika didukung, tata letak seukuran jendela di iPhone) dengan pesan terbaru, kotak chat, reaksi, dan kontrol call di atas video; mengirim pesan bisa dari sana; chat bisa disembunyikan; Esc atau tombolnya untuk keluar |
| 11 | Pasang aplikasi: Chrome/Edge di desktop atau Android lewat "Pasang aplikasi" di landing page; iPhone lewat Bagikan → Tambah ke Layar Utama | Partello terbuka di jendelanya sendiri dengan ikonnya. Saat jaringan mati, membukanya lagi menampilkan aplikasi dari cache, bukan halaman offline browser |
| 12 | Lonceng di header room di perangkat A (izinkan notifikasi; di iPhone/iPad dari aplikasi di Layar Utama, iOS 16.4+), tutup Partello di A, lalu di perangkat B masuk ke room yang kosong itu setidaknya 2 menit setelah server menyala | A menerima "<nama room>: B sedang nonton … Ketuk untuk gabung." dalam bahasa yang dipakai A; mengetuknya membuka room. Tidak ada yang datang jika A sudah membuka room itu di depan layar, dan hanya satu notifikasi per room per 10 menit |

Skenario 11 dan 12 butuh HTTPS atau `http://localhost`, serta build produksi frontend (`npm run build`, disajikan oleh server Rust): service worker tidak didaftarkan oleh dev server Vite. Untuk menjangkau ponsel, pakai tunnel (lihat akhir bagian berikutnya).

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
| 7 | Call dari data seluler dengan Cloudflare TURN (`CLOUDFLARE_TURN_KEY_ID` + `CLOUDFLARE_TURN_API_TOKEN`, tanpa coturn) | Log saat start menunjukkan `turn=cloudflare`; ponsel di data seluler tersambung dengan suara dan kamera dua arah |
| 8 | 4–8 peserta dengan kamera (mesh, tanpa SFU) | Kualitas menurun bertahap (bitrate dibatasi); peserta ke-9 ditolak "call is full" |
| 9 | Call lewat SFU (`LIVEKIT_*` terisi) dengan 3+ kamera, minimal satu di data seluler | Log saat start menunjukkan `calls=sfu`; semua orang melihat semua kamera; lebih dari 8 orang bisa masuk call |
| 10 | Keluar call / tutup tab | Lampu kamera/mic perangkat padam; peer lain melihat peserta keluar dari call |
| 11 | Headset vs speaker laptop | Tidak ada echo (echo cancellation aktif) |
| 12 | Volume per peserta (People → ikon volume) | Hanya suara orang tersebut yang berubah |
| 13 | Speaker laptop (tanpa headset) saat video diputar | Teman mendengar suara Anda jelas; suara video tertekan (voice isolation) dan video mengecil saat ada yang bicara (auto-ducking) |
| 14 | Push-to-talk (ikon tangan) | Teman hanya mendengar Anda selama tombol "Tahan untuk bicara" atau Spasi ditahan; mengetik di chat tidak memicu Spasi |

Untuk uji di jaringan berbeda tanpa deploy, jalankan `./scripts/tunnel.sh` (Cloudflare quick tunnel; isi pengaturan TURN atau LiveKit Cloud di `.env.tunnel`, lihat [DEPLOYMENT.md](DEPLOYMENT.md#6-berbagi-dari-komputer-sendiri-tunnel)). Atau jalankan Partello di balik tunnel HTTPS lain (mis. `cloudflared tunnel --url http://localhost:8080`) dan tambahkan origin tunnel ke `ALLOWED_ORIGINS`.

## Gate desain (Taste Skill)

```bash
node <taste-skill>/scripts/lint.mjs web/src/pages web/src/index.css
node <taste-skill>/scripts/inspect.mjs http://localhost:8080/
```

Hasil terakhir: lint 0 FAIL, inspect 0 FAIL. WARN yang tersisa dijelaskan di README.
