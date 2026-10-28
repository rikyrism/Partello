# Protokol WebSocket Partello

[English](../WEBSOCKET.md) · Bahasa Indonesia

Endpoint: `GET /api/rooms/{slug}/ws` (di produksi `wss://`).

Setiap frame adalah objek JSON dengan field `type`. Tipe Rust ada di `server/src/realtime/protocol.rs`, tipe TypeScript di `web/src/realtime/protocol.ts`; ketiganya (termasuk dokumen ini) harus selalu sinkron.

## Alur koneksi

1. `POST /api/rooms/{slug}/join` → `session_token` (`pts_…`). Kursi dipesan selama 60 detik.
2. Buka WebSocket. Header `Origin` harus termasuk `ALLOWED_ORIGINS`; jika tidak, server membalas 403.
3. Frame **pertama** harus `{"type":"auth","token":"pts_…"}` dalam 10 detik.
4. Server membalas `room_state_sync` berisi seluruh state room. Setelah itu event berjalan dua arah.
5. Kirim `ping` secara berkala (klien Partello: burst saat tersambung, lalu setiap 10 detik) untuk estimasi jam server. Server juga mengirim WebSocket Ping setiap 20 detik dan menutup koneksi yang diam 60 detik.

## Kode penutupan

| Kode | Arti | Tindakan klien |
| --- | --- | --- |
| `4000` | Peserta keluar (`leave`), juga dikirim ke tab lain peserta yang sama | Selesai |
| `4003` | Dikeluarkan host (`kick`) | Tampilkan pesan, hapus sesi |
| `4429` | Sesi ini membuka koneksi ke-4; koneksi **terlama** ditutup (biasanya socket mati di jaringan seluler) | Tampilkan "sudah terbuka di tab lain" |
| `4008` | Klien terlalu lambat membaca (antrean kirim penuh) | Reconnect |
| `4400` | Terlalu banyak frame rusak | Reconnect |
| `4401` | Token tidak valid/kedaluwarsa | Join ulang lalu reconnect |
| `4404` | Room tidak ada | Tampilkan "room tidak ada" |
| `4409` | Room penuh | Tampilkan "room penuh" |
| lainnya | Jaringan | Reconnect dengan exponential backoff (0,5 s → 10 s, jitter) |

## Event klien → server

| `type` | Field | Aturan |
| --- | --- | --- |
| `auth` | `token` | Hanya sebagai frame pertama |
| `ping` | `t` (ms klien) | Dibalas `pong` |
| `chat_send` | `body`, `nonce?`, `reply_to?` (uuid pesan) | 1–500 karakter, tanpa karakter kontrol/bidi; 6 burst, 1/detik. `reply_to` mengutip pesan itu; jika pesan itu sudah tidak ada di riwayat memori (100 pesan terakhir), kutipannya dibuang dan pesan dikirim sebagai pesan biasa |
| `playback_control` | `action`: `play`/`pause`/`seek`, `position` (detik) | Hanya host, atau semua orang jika `control_mode = everyone` |
| `playback_duration` | `item_id`, `duration` | Hanya dari pengontrol playback; laporan pertama untuk item aktif yang diterima |
| `playback_ended` | `item_id` | Dari pengontrol, atau dari siapa pun bila server setuju video hampir habis |
| `queue_add` | `url`, `nonce?` | URL YouTube yang bisa di-embed; sesuai `queue_mode`; maks. 100 item |
| `queue_add_playlist` | `url`, `nonce?` | Link YouTube dengan `list=`; butuh `YOUTUBE_API_KEY` (`limits.playlist_import`). Menambah hingga 50 video yang bisa diputar, sebanyak yang muat di antrean; video pertama langsung diputar jika belum ada yang diputar. Mix YouTube (`list=RD…`) ditolak. Sesuai `queue_mode`. Error (`invalid_video`, `unavailable`, …) menyertakan `nonce` |
| `queue_remove` | `item_id` | Host, atau penambah item jika `queue_mode = everyone` |
| `queue_play` | `item_id` | Pengontrol playback |
| `queue_move` | `item_id`, `to_index` | Pengontrol playback |
| `queue_skip` | – | Pengontrol playback |
| `skip_vote` | `item_id`, `vote` (bool) | Siapa pun; `vote: false` menarik vote. Diabaikan jika `item_id` bukan lagi video yang sedang diputar. Video dilewati pada `floor(n/2)+1` vote dari `n` peserta yang tersambung |
| `start_countdown` | – | Pengontrol playback, saat video sedang diputar atau di-pause (selain itu `conflict`). Mem-pause di posisi saat ini, lalu `countdown`; video diputar untuk semua orang 3 detik kemudian. Lihat `countdown` di bawah |
| `react` | `emoji` | Hanya salah satu dari 😂 🔥 😱 ❤️ 👏 😭 🤯 👀; selain itu `validation_failed` |
| `voice_join` | `muted`, `camera` | Maks. `limits.max_voice_participants` orang dalam call; jika penuh: "The call is full (N people)" |
| `voice_update` | `muted`, `camera` | Harus sudah di call |
| `voice_leave` | – | |
| `voice_signal` | `to`, `data` | Kedua pihak di call; `data.kind` = `description`/`candidate`; maks. 24 KB. Hanya dipakai di mode mesh |
| `host_transfer` | `to` | Hanya host |
| `kick` | `participant_id` | Hanya host; peserta dikeluarkan, sesinya dicabut, tab-nya ditutup dengan `4003` |
| `leave` | – | Keluar segera; sesi dicabut |

Rate limit per koneksi (token bucket): umum 30/detik, playback 4/detik (`playback_control`, `queue_play`, `queue_skip`, `skip_vote`, `start_countdown`), antrean 0,5/detik dengan burst 5 (`queue_add`, `queue_add_playlist`, `queue_remove`, `queue_move`), reaksi 3/detik dengan burst 10, signaling 60/detik (burst 300, terpisah dari anggaran umum). Satu sesi maksimal 3 koneksi. Pelanggaran dibalas `error` dengan `code: "rate_limited"`.

Timestamp di chat dan tombol 📌 tidak butuh apa pun dari protokol: "📌 12:45" adalah `chat_send` biasa, dan mengklik waktu di chat mengirim seek `playback_control` biasa (ditolak seperti biasa untuk orang yang tidak boleh mengontrol playback).

Peserta yang kehilangan semua koneksinya otomatis dikeluarkan dari call (kursi call dibebaskan). Setelah tersambung lagi, klien mengirim `voice_join` ulang.

## Event server → klien

| `type` | Field | Kapan |
| --- | --- | --- |
| `room_state_sync` | `self_id`, `room`, `participants`, `playback`, `queue`, `chat`, `ice_servers`, `limits`, `skip_votes`, `server_time` | Setelah auth (juga setelah reconnect). `skip_votes` = id orang yang memberi vote skip untuk video yang sedang diputar |
| `participant_joined` | `participant` | Peserta pertama kali tersambung |
| `participant_left` | `participant_id`, `display_name`, `reason`: `left`/`disconnected`/`kicked` | Keluar, grace window habis, atau dikeluarkan host |
| `participant_updated` | `participant` | Host berubah (termasuk pengalihan host), status koneksi, status call/mute/kamera |
| `playback_changed` | `playback`, `actor_id`, `server_time`, `nonce?` | Setiap perubahan playback; `nonce` = nonce `queue_add`/`queue_add_playlist` yang dituntaskannya. `actor_id` bernilai `null` untuk perubahan yang tidak dilakukan langsung oleh seseorang (mis. vote skip yang lolos) |
| `queue_updated` | `queue`, `actor_id`, `nonce?` | Setiap perubahan antrean; `nonce` dan `actor_id` seperti di atas |
| `chat_message_created` | `message` (`nonce` milik pengirim disertakan) | Pesan baru |
| `reaction` | `participant_id`, `emoji` | Seseorang mengirim reaksi (ke semua orang, termasuk pengirimnya) |
| `countdown` | `start_at` (ms server), `version`, `actor_id` | Tepat setelah `playback_changed` (pause) yang disebabkan `start_countdown`. Tampilkan 3-2-1 sampai `start_at`. Pada `start_at` server mengirim `playback_changed` (diputar, posisi sama, `actor_id: null`), kecuali playback berubah lebih dulu: `playback_changed` apa pun dengan `version` di atas versi ini membatalkan hitung mundur, dan setelah itu tidak ada lagi yang dikirim untuknya |
| `skip_votes` | `item_id`, `voters` (uuid[]), `skipped` | Vote skip ditambahkan atau ditarik. Jika `skipped: true`, video sudah dilewati dan disusul `queue_updated`/`playback_changed` dengan `actor_id: null` |
| `voice_signal` | `from`, `data` | Signaling WebRTC untuk penerima saja (mode mesh) |
| `call_token` | `url`, `token` | Hanya mode SFU: dikirim hanya ke peserta yang `voice_join`-nya diterima. Sambungkan ke LiveKit di `url` dengan `token` |
| `room_settings_changed` | `room` | Host mengubah pengaturan |
| `pong` | `t`, `server_time` | Balasan `ping` |
| `error` | `code`, `message`, `nonce?` | Hanya ke pengirim event yang ditolak |

### Bentuk data utama

```jsonc
// participant
{ "id": "uuid", "display_name": "Ana", "is_host": true, "connected": true,
  "joined_at": 1793113547445, "voice": { "muted": false, "camera": true } | null }

// playback
{ "item": QueueItem | null, "status": "idle|playing|paused|ended",
  "position": 12.5, "updated_at": 1793113547445, "version": 7, "duration": 634 | null }

// queue item
{ "id": "uuid", "video_id": "aqz-KE-bpKQ", "title": "…", "author": "Blender",
  "thumbnail_url": "https://i.ytimg.com/vi/…/hqdefault.jpg", "start": null,
  "added_by": "uuid", "added_by_name": "Ana" }

// pesan chat (reply_to hanya pada balasan, nonce hanya untuk pengirim)
{ "id": "uuid", "participant_id": "uuid", "display_name": "Budi", "body": "setuju",
  "created_at": 1793113547445,
  "reply_to": { "id": "uuid", "participant_id": "uuid", "display_name": "Ana",
                "body": "kutipan maks. 140 karakter, diberi … jika terpotong" },
  "nonce": "c2" }

// room
{ "slug": "k7Qm2xPa9Rt4", "name": "Nobar",
  "settings": { "control_mode": "host_only", "queue_mode": "everyone", "permanent": false },
  "max_participants": 16, "created_at": 1793113547445, "has_password": false }

// limits
{ "chat_max_chars": 500, "max_voice_participants": 8, "max_queue_len": 100,
  "search_enabled": true, "playlist_import": true, "call_mode": "mesh" | "sfu" }
```

Posisi saat ini dihitung klien: `position + (status == "playing" ? (server_now - updated_at)/1000 : 0)`, dengan `server_now = Date.now() + offset` dari ping/pong.

`limits.call_mode` memberi tahu klien cara menjalankan call: `mesh` (peer-to-peer, signaling lewat `voice_signal`) atau `sfu` (LiveKit, kredensial lewat `call_token`). `limits.playlist_import` dan `limits.search_enabled` bernilai true jika server punya `YOUTUBE_API_KEY`.

## Contoh sesi

```text
→ {"type":"auth","token":"pts_…"}
← {"type":"room_state_sync","self_id":"…","playback":{"status":"idle",…},"skip_votes":[],…}
→ {"type":"queue_add","url":"https://youtu.be/aqz-KE-bpKQ","nonce":"q1"}
← {"type":"playback_changed","playback":{"status":"playing","position":0,"version":1,…},"actor_id":"…","nonce":"q1"}
→ {"type":"playback_control","action":"pause","position":12.5}
← {"type":"playback_changed","playback":{"status":"paused","position":12.5,"version":2,…}}
→ {"type":"chat_send","body":"popcorn ready","nonce":"c1"}
← {"type":"chat_message_created","message":{"body":"popcorn ready","nonce":"c1",…}}
→ {"type":"react","emoji":"🔥"}
← {"type":"reaction","participant_id":"…","emoji":"🔥"}
→ {"type":"skip_vote","item_id":"…","vote":true}
← {"type":"skip_votes","item_id":"…","voters":["…"],"skipped":false}
→ {"type":"start_countdown"}
← {"type":"playback_changed","playback":{"status":"paused","position":40.2,"version":3,…},"actor_id":"…"}
← {"type":"countdown","start_at":1793113550445,"version":3,"actor_id":"…"}
← {"type":"playback_changed","playback":{"status":"playing","position":40.2,"version":4,…},"actor_id":null}
```
