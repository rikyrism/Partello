# Partello developer API (`/api/v1`)

English · [Bahasa Indonesia](id/API.md)

This API lets other products (websites, Discord/Telegram bots, apps) open a Partello room and control its video. The full schema is in [openapi.yaml](openapi.yaml), and the interactive guide is at `/developers` in the app.

## Authentication

1. Create a key: `POST /api/v1/keys` with `{"label":"My bot"}`, or through the `/developers` page. The key (`ptk_…`) is **shown only once**; the server stores only its SHA-256 hash.
2. Send `Authorization: Bearer ptk_…` **from your server**. Do not put the key in browser code.
3. Limits: 60 requests/minute per key; key creation 5/hour per IP.
4. A key can only access the rooms it created. Rooms owned by another key answer 404.

## Endpoints

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/api/v1/keys` | Create an API key |
| `POST` | `/api/v1/rooms` | Create a room (optionally playing `video_url` right away) |
| `GET` | `/api/v1/rooms/{slug}` | Status: what is playing, position, queue, participant count |
| `POST` | `/api/v1/rooms/{slug}/play` | Replace the video that is playing now |
| `POST` | `/api/v1/rooms/{slug}/queue` | Add 1–50 videos to the queue |

## Room settings

`settings` in `POST /api/v1/rooms` is optional, and so is each of its fields. The same object comes back as `room.settings` (RoomInfo) in every response that contains a room.

| Field | Values | Default | Meaning |
| --- | --- | --- | --- |
| `control_mode` | `host_only` / `everyone` | `host_only` | Who may play, pause, seek, and skip |
| `queue_mode` | `everyone` / `host_only` | `everyone` | Who may add videos |
| `permanent` | `true` / `false` | `false` | Keep the room forever: it is never deleted for inactivity (`ROOM_RETENTION_DAYS`), so the link keeps working |

Later changes are made by the room's host in the app (settings dialog, "Keep this room forever"), which calls `PATCH /api/rooms/{slug}/settings` with the host's session token. Its body takes any of `name`, `control_mode`, `queue_mode`, `permanent`, and `pin` (an empty string removes the PIN); it answers with the updated RoomInfo, and everyone in the room receives `room_settings_changed`.

## Example

```bash
KEY=ptk_xxx
BASE=http://localhost:8080

# A room that starts playing right away, everyone may control it, kept forever
curl -s -X POST $BASE/api/v1/rooms \
  -H "Authorization: Bearer $KEY" -H "Content-Type: application/json" \
  -d '{"name":"Trailer night","video_url":"https://youtu.be/aqz-KE-bpKQ","settings":{"control_mode":"everyone","permanent":true},"pin":"0420"}'
# → { "room": {...}, "invite_url": ".../r/k7Qm2xPa9Rt4", "host_key": "phk_…", "host_url": ".../r/k7Qm2xPa9Rt4#host=phk_…" }

curl -s -X POST $BASE/api/v1/rooms/k7Qm2xPa9Rt4/queue \
  -H "Authorization: Bearer $KEY" -H "Content-Type: application/json" \
  -d '{"items":[{"url":"https://youtu.be/9bZkp7q19f0"},{"url":"https://youtu.be/kJQP7kiw5Fk"}]}'

curl -s $BASE/api/v1/rooms/k7Qm2xPa9Rt4 -H "Authorization: Bearer $KEY"
```

- `invite_url`: share with all viewers.
- `pin` (optional): 4 digits viewers must enter; `host_url` does not need it.
- `host_url`: give it only to the person who will be host. The `#host=` fragment is read by the browser, stored, then removed from the address bar, and is never sent as part of an HTTP request.

## Errors

```json
{ "error": { "code": "invalid_video", "message": "The owner of this video doesn't allow playing it on other sites" } }
```

| HTTP | `code` |
| --- | --- |
| 400 / 415 | `validation_failed` (malformed JSON / wrong content type) |
| 401 | `unauthorized` |
| 404 | `room_not_found` |
| 409 | `room_full`, `conflict` |
| 422 | `validation_failed`, `invalid_video` |
| 429 | `rate_limited` |
| 503 | `unavailable` |
