# API Developer Partello (`/api/v1`)

[English](../API.md) · Bahasa Indonesia

API ini memungkinkan produk lain (situs, bot Discord/Telegram, aplikasi) membuka room Partello dan mengatur videonya. Skema lengkap ada di [openapi.yaml](../openapi.yaml), dan halaman panduan interaktifnya di `/developers` pada aplikasi.

## Autentikasi

1. Buat key: `POST /api/v1/keys` dengan `{"label":"Bot saya"}`, atau lewat halaman `/developers`. Key (`ptk_…`) **hanya ditampilkan sekali**; server menyimpan hash SHA-256-nya saja.
2. Kirim `Authorization: Bearer ptk_…` **dari server Anda**. Jangan menaruh key di kode browser.
3. Batas: 60 request/menit per key; pembuatan key 5/jam per IP.
4. Satu key hanya bisa mengakses room yang ia buat. Room milik key lain dijawab 404.

## Endpoint

| Method | Path | Fungsi |
| --- | --- | --- |
| `POST` | `/api/v1/keys` | Membuat API key |
| `POST` | `/api/v1/rooms` | Membuat room (opsional langsung memutar `video_url`) |
| `GET` | `/api/v1/rooms/{slug}` | Status: yang diputar, posisi, antrean, jumlah peserta |
| `POST` | `/api/v1/rooms/{slug}/play` | Mengganti video yang diputar sekarang |
| `POST` | `/api/v1/rooms/{slug}/queue` | Menambah 1–50 video ke antrean |

## Pengaturan room

`settings` pada `POST /api/v1/rooms` bersifat opsional, begitu juga setiap field-nya. Objek yang sama dikembalikan sebagai `room.settings` (RoomInfo) di setiap respons yang berisi room.

| Field | Nilai | Default | Arti |
| --- | --- | --- | --- |
| `control_mode` | `host_only` / `everyone` | `host_only` | Siapa yang boleh play, pause, seek, dan skip |
| `queue_mode` | `everyone` / `host_only` | `everyone` | Siapa yang boleh menambah video |
| `permanent` | `true` / `false` | `false` | Simpan room selamanya: room tidak pernah dihapus karena tidak aktif (`ROOM_RETENTION_DAYS`), sehingga link-nya tetap berlaku |

Perubahan berikutnya dilakukan oleh host room di aplikasi (dialog pengaturan, "Keep this room forever"), yang memanggil `PATCH /api/rooms/{slug}/settings` dengan session token host. Body-nya menerima `name`, `control_mode`, `queue_mode`, `permanent`, dan `pin` (string kosong menghapus PIN), masing-masing opsional; balasannya RoomInfo yang sudah diperbarui, dan semua orang di room menerima `room_settings_changed`.

## Notifikasi push (aplikasi web)

Tombol lonceng "beri tahu saya" di aplikasi memakai tiga endpoint di luar `/api/v1`. Endpoint ini tidak memakai API key: dua endpoint room memakai session token peserta (`Authorization: Bearer pts_…`, dari `POST /api/rooms/{slug}/join`). Cara notifikasi dikirim dijelaskan di [ARCHITECTURE.md](ARCHITECTURE.md#notifikasi-push).

| Method | Path | Fungsi |
| --- | --- | --- |
| `GET` | `/api/push/key` | `{"public_key": "…"}`: public key VAPID server (base64url) untuk `PushManager.subscribe({ applicationServerKey })` |
| `POST` | `/api/rooms/{slug}/push` | Body `{"subscription": <PushSubscription.toJSON()>, "lang": "id"}` (`lang`: `en` atau `id`). Beri tahu browser ini saat ada yang mulai nonton di room; dijawab 204. Subscribe lagi memperbarui key dan bahasanya |
| `DELETE` | `/api/rooms/{slug}/push` | Body `{"endpoint": "…"}`. Hentikan notifikasi untuk room ini; dijawab 204 |

- Hanya endpoint `https` di layanan push browser (Google FCM, Mozilla, Apple, Windows) yang diterima; selain itu `validation_failed` (422).
- Paling banyak satu notifikasi per room setiap 10 menit, ditulis dalam `lang` (selain `id` berarti bahasa Inggris). Satu room menyimpan maksimal 200 subscription; lebih dari itu, yang paling lama dibuang.
- `POST` dihitung dalam batas per IP yang sama dengan bergabung ke room. Jika push tidak aktif di server, `GET /api/push/key` dan `POST` menjawab `unavailable` (503).

## Contoh

```bash
KEY=ptk_xxx
BASE=http://localhost:8080

# Room yang langsung memutar video, semua orang boleh mengontrol, disimpan selamanya
curl -s -X POST $BASE/api/v1/rooms \
  -H "Authorization: Bearer $KEY" -H "Content-Type: application/json" \
  -d '{"name":"Nobar trailer","video_url":"https://youtu.be/aqz-KE-bpKQ","settings":{"control_mode":"everyone","permanent":true},"pin":"0420"}'
# → { "room": {...}, "invite_url": ".../r/k7Qm2xPa9Rt4", "host_key": "phk_…", "host_url": ".../r/k7Qm2xPa9Rt4#host=phk_…" }

curl -s -X POST $BASE/api/v1/rooms/k7Qm2xPa9Rt4/queue \
  -H "Authorization: Bearer $KEY" -H "Content-Type: application/json" \
  -d '{"items":[{"url":"https://youtu.be/9bZkp7q19f0"},{"url":"https://youtu.be/kJQP7kiw5Fk"}]}'

curl -s $BASE/api/v1/rooms/k7Qm2xPa9Rt4 -H "Authorization: Bearer $KEY"
```

- `invite_url`: bagikan ke semua penonton.
- `pin` (opsional): 4 angka yang wajib diisi penonton; `host_url` tidak memerlukannya.
- `host_url`: berikan hanya kepada orang yang akan menjadi host. Fragmen `#host=` dibaca browser, disimpan, lalu dihapus dari address bar, dan tidak pernah dikirim sebagai bagian request HTTP.

## Error

```json
{ "error": { "code": "invalid_video", "message": "The owner of this video doesn't allow playing it on other sites" } }
```

| HTTP | `code` |
| --- | --- |
| 400 / 415 | `validation_failed` (JSON rusak / content-type salah) |
| 401 | `unauthorized` |
| 404 | `room_not_found` |
| 409 | `room_full`, `conflict` |
| 422 | `validation_failed`, `invalid_video` |
| 429 | `rate_limited` |
| 503 | `unavailable` |
