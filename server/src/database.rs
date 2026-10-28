//! Persistence. `Store` is an enum rather than a trait object so both backends
//! keep plain `async fn`s: PostgreSQL in every real deployment, and an
//! in-memory map for tests and quick local demos without a database.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::room::model::{ChatMessage, ControlMode, QueueMode, ReplyRef, RoomRecord, RoomSettings};

#[derive(Debug, Clone)]
pub struct ApiKeyRecord {
    pub id: Uuid,
    pub label: String,
    pub revoked: bool,
}

#[derive(Clone)]
pub enum Store {
    Postgres(PgPool),
    Memory(Arc<Mutex<MemData>>),
}

#[derive(Default)]
pub struct MemData {
    rooms: HashMap<String, (RoomRecord, i64)>,
    saved_state: HashMap<Uuid, String>,
    chat: Vec<(Uuid, ChatMessage)>,
    api_keys: Vec<(ApiKeyRecord, Vec<u8>)>,
}

fn ms(t: OffsetDateTime) -> i64 {
    (t.unix_timestamp_nanos() / 1_000_000) as i64
}

fn from_ms(ms: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000).unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

pub fn now_ms() -> i64 {
    ms(OffsetDateTime::now_utc())
}

impl Store {
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Store::Postgres(pool))
    }

    pub fn memory() -> Self {
        Store::Memory(Arc::default())
    }

    pub async fn ping(&self) -> Result<(), sqlx::Error> {
        match self {
            Store::Postgres(pool) => sqlx::query("SELECT 1").execute(pool).await.map(|_| ()),
            Store::Memory(_) => Ok(()),
        }
    }

    /// Returns false when the slug is already taken (caller retries with a new one).
    pub async fn insert_room(&self, r: &RoomRecord) -> Result<bool, sqlx::Error> {
        match self {
            Store::Postgres(pool) => {
                let res = sqlx::query(
                    "INSERT INTO rooms (id, slug, name, host_key_hash, control_mode, queue_mode, max_participants, api_key_id, created_at, last_active_at, password_hash, permanent)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9, $10, $11)
                     ON CONFLICT (slug) DO NOTHING",
                )
                .bind(r.id)
                .bind(&r.slug)
                .bind(&r.name)
                .bind(&r.host_key_hash)
                .bind(r.settings.control_mode.as_str())
                .bind(r.settings.queue_mode.as_str())
                .bind(r.max_participants as i32)
                .bind(r.api_key_id)
                .bind(from_ms(r.created_at))
                .bind(&r.password_hash)
                .bind(r.settings.permanent)
                .execute(pool)
                .await?;
                Ok(res.rows_affected() == 1)
            }
            Store::Memory(m) => {
                let mut m = m.lock().unwrap();
                if m.rooms.contains_key(&r.slug) {
                    return Ok(false);
                }
                m.rooms.insert(r.slug.clone(), (r.clone(), r.created_at));
                Ok(true)
            }
        }
    }

    pub async fn room_by_slug(&self, slug: &str) -> Result<Option<RoomRecord>, sqlx::Error> {
        match self {
            Store::Postgres(pool) => {
                let row = sqlx::query(
                    "SELECT id, slug, name, host_key_hash, control_mode, queue_mode, max_participants, api_key_id, created_at, password_hash, permanent
                     FROM rooms WHERE slug = $1",
                )
                .bind(slug)
                .fetch_optional(pool)
                .await?;
                row.map(|row| {
                    Ok(RoomRecord {
                        id: row.try_get("id")?,
                        slug: row.try_get("slug")?,
                        name: row.try_get("name")?,
                        host_key_hash: row.try_get("host_key_hash")?,
                        settings: RoomSettings {
                            control_mode: ControlMode::parse(row.try_get("control_mode")?),
                            queue_mode: QueueMode::parse(row.try_get("queue_mode")?),
                            permanent: row.try_get("permanent")?,
                        },
                        max_participants: row.try_get::<i32, _>("max_participants")? as u32,
                        api_key_id: row.try_get("api_key_id")?,
                        created_at: ms(row.try_get("created_at")?),
                        password_hash: row.try_get("password_hash")?,
                    })
                })
                .transpose()
            }
            Store::Memory(m) => Ok(m.lock().unwrap().rooms.get(slug).map(|(r, _)| r.clone())),
        }
    }

    pub async fn update_room(&self, id: Uuid, name: &str, settings: RoomSettings, password_hash: Option<&str>) -> Result<(), sqlx::Error> {
        match self {
            Store::Postgres(pool) => {
                sqlx::query(
                    "UPDATE rooms SET name = $2, control_mode = $3, queue_mode = $4, password_hash = $5, permanent = $6, last_active_at = now() WHERE id = $1",
                )
                .bind(id)
                .bind(name)
                .bind(settings.control_mode.as_str())
                .bind(settings.queue_mode.as_str())
                .bind(password_hash)
                .bind(settings.permanent)
                .execute(pool)
                .await?;
            }
            Store::Memory(m) => {
                if let Some((r, _)) = m.lock().unwrap().rooms.values_mut().find(|(r, _)| r.id == id) {
                    r.name = name.to_string();
                    r.settings = settings;
                    r.password_hash = password_hash.map(String::from);
                }
            }
        }
        Ok(())
    }

    /// Stores the live state of a room (JSON from `SavedState`).
    pub async fn save_room_state(&self, id: Uuid, json: &str) -> Result<(), sqlx::Error> {
        match self {
            Store::Postgres(pool) => {
                sqlx::query("UPDATE rooms SET saved_state = $2::jsonb WHERE id = $1")
                    .bind(id)
                    .bind(json)
                    .execute(pool)
                    .await?;
            }
            Store::Memory(m) => {
                m.lock().unwrap().saved_state.insert(id, json.to_string());
            }
        }
        Ok(())
    }

    pub async fn room_state(&self, id: Uuid) -> Result<Option<String>, sqlx::Error> {
        match self {
            Store::Postgres(pool) => {
                let row = sqlx::query("SELECT saved_state::text AS s FROM rooms WHERE id = $1")
                    .bind(id)
                    .fetch_optional(pool)
                    .await?;
                Ok(row.and_then(|r| r.try_get::<Option<String>, _>("s").ok().flatten()))
            }
            Store::Memory(m) => Ok(m.lock().unwrap().saved_state.get(&id).cloned()),
        }
    }

    pub async fn touch_rooms(&self, ids: &[Uuid]) -> Result<(), sqlx::Error> {
        if ids.is_empty() {
            return Ok(());
        }
        match self {
            Store::Postgres(pool) => {
                sqlx::query("UPDATE rooms SET last_active_at = now() WHERE id = ANY($1)")
                    .bind(ids)
                    .execute(pool)
                    .await?;
            }
            Store::Memory(m) => {
                let now = now_ms();
                for (r, active) in m.lock().unwrap().rooms.values_mut() {
                    if ids.contains(&r.id) {
                        *active = now;
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn insert_chat(&self, room_id: Uuid, msg: &ChatMessage) -> Result<(), sqlx::Error> {
        match self {
            Store::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO chat_messages (id, room_id, participant_id, display_name, body, created_at, reply_to)
                     VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb)",
                )
                .bind(msg.id)
                .bind(room_id)
                .bind(msg.participant_id)
                .bind(&msg.display_name)
                .bind(&msg.body)
                .bind(from_ms(msg.created_at))
                .bind(msg.reply_to.as_ref().and_then(|r| serde_json::to_string(r).ok()))
                .execute(pool)
                .await?;
            }
            Store::Memory(m) => m.lock().unwrap().chat.push((room_id, msg.clone())),
        }
        Ok(())
    }

    /// The latest `limit` messages, oldest first.
    pub async fn recent_chat(&self, room_id: Uuid, limit: usize) -> Result<Vec<ChatMessage>, sqlx::Error> {
        match self {
            Store::Postgres(pool) => {
                let rows = sqlx::query(
                    "SELECT id, participant_id, display_name, body, created_at, reply_to::text AS reply_to FROM chat_messages
                     WHERE room_id = $1 ORDER BY created_at DESC LIMIT $2",
                )
                .bind(room_id)
                .bind(limit as i64)
                .fetch_all(pool)
                .await?;
                let mut out = rows
                    .into_iter()
                    .map(|row| {
                        Ok(ChatMessage {
                            id: row.try_get("id")?,
                            participant_id: row.try_get("participant_id")?,
                            display_name: row.try_get("display_name")?,
                            body: row.try_get("body")?,
                            created_at: ms(row.try_get("created_at")?),
                            reply_to: row
                                .try_get::<Option<String>, _>("reply_to")?
                                .and_then(|j| serde_json::from_str::<ReplyRef>(&j).ok()),
                            nonce: None,
                        })
                    })
                    .collect::<Result<Vec<_>, sqlx::Error>>()?;
                out.reverse();
                Ok(out)
            }
            Store::Memory(m) => {
                let m = m.lock().unwrap();
                let all: Vec<_> = m.chat.iter().filter(|(r, _)| *r == room_id).map(|(_, c)| c.clone()).collect();
                Ok(all[all.len().saturating_sub(limit)..].to_vec())
            }
        }
    }

    /// Deletes rooms (and, by cascade, their chat) inactive since `cutoff_ms`.
    pub async fn delete_inactive_rooms(&self, cutoff_ms: i64) -> Result<u64, sqlx::Error> {
        match self {
            Store::Postgres(pool) => {
                let res = sqlx::query("DELETE FROM rooms WHERE last_active_at < $1 AND NOT permanent")
                    .bind(from_ms(cutoff_ms))
                    .execute(pool)
                    .await?;
                Ok(res.rows_affected())
            }
            Store::Memory(m) => {
                let mut m = m.lock().unwrap();
                let before = m.rooms.len();
                m.rooms.retain(|_, (r, active)| r.settings.permanent || *active >= cutoff_ms);
                let alive: Vec<Uuid> = m.rooms.values().map(|(r, _)| r.id).collect();
                m.chat.retain(|(room, _)| alive.contains(room));
                Ok((before - m.rooms.len()) as u64)
            }
        }
    }

    pub async fn insert_api_key(&self, rec: &ApiKeyRecord, key_hash: &[u8], prefix: &str) -> Result<(), sqlx::Error> {
        match self {
            Store::Postgres(pool) => {
                sqlx::query("INSERT INTO api_keys (id, label, key_hash, key_prefix) VALUES ($1, $2, $3, $4)")
                    .bind(rec.id)
                    .bind(&rec.label)
                    .bind(key_hash)
                    .bind(prefix)
                    .execute(pool)
                    .await?;
            }
            Store::Memory(m) => m.lock().unwrap().api_keys.push((rec.clone(), key_hash.to_vec())),
        }
        Ok(())
    }

    pub async fn api_key_by_hash(&self, key_hash: &[u8]) -> Result<Option<ApiKeyRecord>, sqlx::Error> {
        match self {
            Store::Postgres(pool) => {
                let row = sqlx::query(
                    "UPDATE api_keys SET last_used_at = now() WHERE key_hash = $1 RETURNING id, label, revoked_at IS NOT NULL AS revoked",
                )
                .bind(key_hash)
                .fetch_optional(pool)
                .await?;
                row.map(|row| {
                    Ok(ApiKeyRecord {
                        id: row.try_get("id")?,
                        label: row.try_get("label")?,
                        revoked: row.try_get("revoked")?,
                    })
                })
                .transpose()
            }
            Store::Memory(m) => Ok(m
                .lock()
                .unwrap()
                .api_keys
                .iter()
                .find(|(_, h)| crate::security::digest_eq(h, key_hash))
                .map(|(r, _)| r.clone())),
        }
    }

    #[cfg(test)]
    pub fn mem_set_last_active(&self, slug: &str, at: i64) {
        if let Store::Memory(m) = self {
            if let Some((_, active)) = m.lock().unwrap().rooms.get_mut(slug) {
                *active = at;
            }
        }
    }
}
