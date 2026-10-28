//! Store tests against a real PostgreSQL. Skipped unless `TEST_DATABASE_URL`
//! is set (CI sets it; locally: `docker compose up -d postgres`).

use partello::{
    database::{ApiKeyRecord, Store, now_ms},
    room::model::{ChatMessage, ControlMode, QueueMode, ReplyRef, RoomRecord, RoomSettings},
    security,
};
use uuid::Uuid;

async fn store() -> Option<Store> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    Some(Store::connect(&url).await.expect("connect + migrate"))
}

fn record() -> RoomRecord {
    RoomRecord {
        id: Uuid::new_v4(),
        slug: security::new_room_slug(),
        name: "Movie night".into(),
        host_key_hash: security::hash_secret("phk_test"),
        settings: RoomSettings::default(),
        max_participants: 16,
        api_key_id: None,
        created_at: now_ms(),
        password_hash: Some("$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".into()),
    }
}

#[tokio::test]
async fn rooms_chat_and_keys_round_trip() {
    let Some(store) = store().await else {
        eprintln!("TEST_DATABASE_URL not set; skipping");
        return;
    };
    store.ping().await.unwrap();

    let room = record();
    assert!(store.insert_room(&room).await.unwrap());
    assert!(!store.insert_room(&room).await.unwrap(), "duplicate slug is refused");

    let loaded = store.room_by_slug(&room.slug).await.unwrap().unwrap();
    assert_eq!(loaded.id, room.id);
    assert_eq!(loaded.host_key_hash, room.host_key_hash);
    assert_eq!(loaded.settings, RoomSettings::default());
    assert!(loaded.password_hash.is_some());

    let settings = RoomSettings {
        control_mode: ControlMode::Everyone,
        queue_mode: QueueMode::HostOnly,
        permanent: false,
    };
    store.update_room(room.id, "Renamed", settings, None).await.unwrap();
    let loaded = store.room_by_slug(&room.slug).await.unwrap().unwrap();
    assert_eq!(loaded.name, "Renamed");
    assert_eq!(loaded.settings, settings);
    assert_eq!(loaded.password_hash, None, "password removed");

    for i in 0..5 {
        let msg = ChatMessage {
            id: Uuid::new_v4(),
            participant_id: Uuid::new_v4(),
            display_name: "Ana".into(),
            body: format!("message {i}"),
            created_at: now_ms() + i,
            reply_to: (i == 4).then(|| ReplyRef {
                id: Uuid::new_v4(),
                participant_id: Uuid::new_v4(),
                display_name: "Riky".into(),
                body: "what are we watching?".into(),
            }),
            nonce: None,
        };
        store.insert_chat(room.id, &msg).await.unwrap();
    }
    let recent = store.recent_chat(room.id, 3).await.unwrap();
    assert_eq!(
        recent.iter().map(|m| m.body.as_str()).collect::<Vec<_>>(),
        ["message 2", "message 3", "message 4"]
    );
    assert_eq!(
        recent[2].reply_to.as_ref().unwrap().display_name,
        "Riky",
        "replies keep their quote"
    );
    assert!(recent[1].reply_to.is_none());

    assert_eq!(store.room_state(room.id).await.unwrap(), None);
    store.save_room_state(room.id, r#"{"queue":[],"saved_at":1}"#).await.unwrap();
    let saved: serde_json::Value = serde_json::from_str(&store.room_state(room.id).await.unwrap().unwrap()).unwrap();
    assert_eq!(saved["saved_at"], 1);

    // Server settings keep the first value written.
    let first = store.setting_or_insert("test_key", &format!("a-{}", room.id)).await.unwrap();
    assert_eq!(store.setting_or_insert("test_key", "b").await.unwrap(), first);

    let sub = partello::push::Subscription {
        endpoint: format!("https://fcm.googleapis.com/fcm/send/{}", room.id),
        keys: partello::push::SubscriptionKeys {
            p256dh: "BPub".into(),
            auth: "auth".into(),
        },
    };
    store.add_push(room.id, &sub, "id").await.unwrap();
    store.add_push(room.id, &sub, "en").await.unwrap();
    let subs = store.push_subscriptions(room.id).await.unwrap();
    assert_eq!(subs.len(), 1, "re-subscribing updates in place");
    assert_eq!(subs[0].1, "en");
    store.remove_push(room.id, &sub.endpoint).await.unwrap();
    assert!(store.push_subscriptions(room.id).await.unwrap().is_empty());

    let key = ApiKeyRecord {
        id: Uuid::new_v4(),
        label: "bot".into(),
        revoked: false,
    };
    let hash = security::hash_secret(&format!("ptk_{}", key.id));
    store.insert_api_key(&key, &hash, "ptk_abcd").await.unwrap();
    assert_eq!(store.api_key_by_hash(&hash).await.unwrap().unwrap().id, key.id);
    assert!(store.api_key_by_hash(&security::hash_secret("nope")).await.unwrap().is_none());

    store.touch_rooms(&[room.id]).await.unwrap();
    // Nothing is older than one hour ago, so this room survives...
    store.delete_inactive_rooms(now_ms() - 3_600_000).await.unwrap();
    assert!(store.room_by_slug(&room.slug).await.unwrap().is_some());
    // ...and a cutoff in the future deletes it together with its chat.
    store.delete_inactive_rooms(now_ms() + 60_000).await.unwrap();
    assert!(store.room_by_slug(&room.slug).await.unwrap().is_none());
    assert!(store.recent_chat(room.id, 10).await.unwrap().is_empty());

    // Permanent rooms are never purged.
    let mut kept = record();
    kept.settings.permanent = true;
    assert!(store.insert_room(&kept).await.unwrap());
    store.delete_inactive_rooms(now_ms() + 60_000).await.unwrap();
    assert!(store.room_by_slug(&kept.slug).await.unwrap().unwrap().settings.permanent);
}
