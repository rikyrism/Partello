# Protokol WebSocket Partello

Endpoint: `GET /api/rooms/{slug}/ws` (di produksi `wss://`).

Setiap frame adalah objek JSON dengan field `type`. Tipe Rust ada di `server/src/realtime/protocol.rs`, tipe TypeScript di `web/src/realtime/protocol.ts`; ketiganya harus selalu sinkron.

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
| `chat_send` | `body`, `nonce?` | 1–500 karakter, tanpa karakter kontrol/bidi; 6 burst, 1/detik |
| `playback_control` | `action`: `play`/`pause`/`seek`, `position` (detik) | Hanya host, atau semua orang jika `control_mode = everyone` |
| `playback_duration` | `item_id`, `duration` | Hanya dari pengontrol playback; laporan pertama untuk item aktif yang diterima |
| `playback_ended` | `item_id` | Dari pengontrol, atau dari siapa pun bila server setuju video hampir habis |
| `queue_add` | `url`, `nonce?` | URL YouTube yang bisa di-embed; sesuai `queue_mode`; maks. 100 item |
| `queue_remove` | `item_id` | Host, atau penambah item jika `queue_mode = everyone` |
| `queue_play` | `item_id` | Pengontrol playback |
| `queue_move` | `item_id`, `to_index` | Pengontrol playback |
| `queue_skip` | – | Pengontrol playback |
| `voice_join` | `muted`, `camera` | Maks. 8 orang dalam call |
| `voice_update` | `muted`, `camera` | Harus sudah di call |
| `voice_leave` | – | |
| `voice_signal` | `to`, `data` | Kedua pihak di call; `data.kind` = `description`/`candidate`; maks. 24 KB |
| `host_transfer` | `to` | Hanya host |
| `kick` | `participant_id` | Hanya host; peserta dikeluarkan, sesinya dicabut, tab-nya ditutup dengan `4003` |
| `leave` | – | Keluar segera; sesi dicabut |

Rate limit per koneksi (token bucket): umum 30/detik, playback 4/detik, antrean 0,5/detik (burst 5), signaling 60/detik (burst 300, terpisah dari anggaran umum). Satu sesi maksimal 3 koneksi. Pelanggaran dibalas `error` dengan `code: "rate_limited"`.

Peserta yang kehilangan semua koneksinya otomatis dikeluarkan dari call (kursi call dibebaskan). Setelah tersambung lagi, klien mengirim `voice_join` ulang.

## Event server → klien

| `type` | Field | Kapan |
| --- | --- | --- |
| `room_state_sync` | `self_id`, `room`, `participants`, `playback`, `queue`, `chat`, `ice_servers`, `limits`, `server_time` | Setelah auth (juga setelah reconnect) |
| `participant_joined` | `participant` | Peserta pertama kali tersambung |
| `participant_left` | `participant_id`, `display_name`, `reason`: `left`/`disconnected`/`kicked` | Keluar, grace window habis, atau dikeluarkan host |
| `participant_updated` | `participant` | Host berubah, status koneksi, status call/mute/kamera |
| `playback_changed` | `playback`, `actor_id`, `server_time`, `nonce?` | Setiap perubahan playback; `nonce` = nonce `queue_add` yang dituntaskannya |
| `queue_updated` | `queue`, `actor_id`, `nonce?` | Setiap perubahan antrean; `nonce` seperti di atas |
| `chat_message_created` | `message` (`nonce` milik pengirim disertakan) | Pesan baru |
| `voice_signal` | `from`, `data` | Signaling WebRTC untuk penerima saja |
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
```

Posisi saat ini dihitung klien: `position + (status == "playing" ? (server_now - updated_at)/1000 : 0)`, dengan `server_now = Date.now() + offset` dari ping/pong.

## Contoh sesi

```text
→ {"type":"auth","token":"pts_…"}
← {"type":"room_state_sync","self_id":"…","playback":{"status":"idle",…},…}
→ {"type":"queue_add","url":"https://youtu.be/aqz-KE-bpKQ","nonce":"q1"}
← {"type":"playback_changed","playback":{"status":"playing","position":0,"version":1,…},"actor_id":"…"}
→ {"type":"playback_control","action":"pause","position":12.5}
← {"type":"playback_changed","playback":{"status":"paused","position":12.5,"version":2,…}}
→ {"type":"chat_send","body":"popcorn ready","nonce":"c1"}
← {"type":"chat_message_created","message":{"body":"popcorn ready","nonce":"c1",…}}
```
