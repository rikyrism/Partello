use uuid::Uuid;

use super::{
    model::{ControlMode, QueueMode, RoomRecord, RoomSettings, VoiceState},
    playback::PlayStatus,
    queue::QueueItem,
    state::{Effect, RoomLimits, RoomState},
};
use crate::{
    error::AppError,
    realtime::protocol::{PlaybackAction, ServerEvent},
};

const GRACE: i64 = 10_000;
const RESERVE_TTL: i64 = 60_000;

fn room(max: u32) -> RoomState {
    let record = RoomRecord {
        id: Uuid::new_v4(),
        slug: "abcdefghijkm".into(),
        name: "Test".into(),
        host_key_hash: vec![],
        settings: RoomSettings::default(),
        max_participants: max,
        api_key_id: None,
        created_at: 0,
        password_hash: None,
    };
    let limits = RoomLimits {
        max_voice: 2,
        max_queue: 3,
        chat_history: 3,
        chat_max_chars: 500,
        search_enabled: false,
        call_mode: crate::realtime::protocol::CallMode::Mesh,
    };
    RoomState::new(&record, limits, vec![], 0)
}

fn join(r: &mut RoomState, name: &str, creator: bool) -> Uuid {
    let id = Uuid::new_v4();
    r.reserve(id, name.into(), creator, 0).unwrap();
    r.connect(id, 0).unwrap();
    id
}

fn item(n: u8, by: Uuid) -> QueueItem {
    QueueItem {
        id: Uuid::from_bytes([n; 16]),
        video_id: format!("video{n:06}"),
        title: format!("Video {n}"),
        author: None,
        thumbnail_url: String::new(),
        start: None,
        added_by: by,
        added_by_name: "x".into(),
    }
}

fn kinds(fx: &[Effect]) -> Vec<&'static str> {
    fx.iter()
        .map(|e| {
            let ev = match e {
                Effect::Broadcast(ev) | Effect::BroadcastExcept(_, ev) | Effect::To(_, ev) => ev,
                Effect::PersistChat(_) => return "persist_chat",
            };
            match ev {
                ServerEvent::ParticipantJoined { .. } => "participant_joined",
                ServerEvent::ParticipantLeft { .. } => "participant_left",
                ServerEvent::ParticipantUpdated { .. } => "participant_updated",
                ServerEvent::PlaybackChanged { .. } => "playback_changed",
                ServerEvent::QueueUpdated { .. } => "queue_updated",
                ServerEvent::ChatMessageCreated { .. } => "chat_message_created",
                ServerEvent::VoiceSignal { .. } => "voice_signal",
                ServerEvent::RoomSettingsChanged { .. } => "room_settings_changed",
                ServerEvent::Reaction { .. } => "reaction",
                ServerEvent::SkipVotes { .. } => "skip_votes",
                _ => "other",
            }
        })
        .collect()
}

#[test]
fn first_member_becomes_host_and_capacity_is_enforced() {
    let mut r = room(2);
    let a = join(&mut r, "A", false);
    let _b = join(&mut r, "B", false);
    assert_eq!(r.host_id(), Some(a));
    let err = r.reserve(Uuid::new_v4(), "C".into(), false, 0).unwrap_err();
    assert!(matches!(err, AppError::RoomFull));
}

#[test]
fn reservations_are_invisible_until_connected() {
    let mut r = room(4);
    let a = join(&mut r, "A", false);
    let ghost = Uuid::new_v4();
    r.reserve(ghost, "Ghost".into(), false, 0).unwrap();
    assert_eq!(r.participants().len(), 1);
    assert!(matches!(r.chat(ghost, "hi", None, None, 0), Err(AppError::Unauthorized)));
    // Unused reservations expire and free the seat silently.
    let fx = r.sweep(RESERVE_TTL, GRACE, RESERVE_TTL);
    assert!(fx.is_empty());
    assert_eq!(r.occupancy(), 1);
    assert!(r.is_member(a));
}

#[test]
fn creator_reclaims_host_and_host_passes_on_leave() {
    let mut r = room(4);
    let guest = join(&mut r, "Guest", false);
    assert_eq!(r.host_id(), Some(guest));
    let creator = join(&mut r, "Creator", true);
    assert_eq!(r.host_id(), Some(creator), "creator takes host on arrival");
    let fx = r.leave(creator);
    assert_eq!(kinds(&fx), ["participant_left", "participant_updated"]);
    assert_eq!(r.host_id(), Some(guest));
}

#[test]
fn disconnect_keeps_member_for_grace_window() {
    let mut r = room(4);
    let a = join(&mut r, "A", false);
    let b = join(&mut r, "B", false);
    let fx = r.disconnect(b, 1_000);
    assert_eq!(kinds(&fx), ["participant_updated"]);
    assert!(!r.participant(b).unwrap().connected);

    // Reconnect within the grace window: same identity, no leave event.
    assert!(r.sweep(5_000, GRACE, RESERVE_TTL).is_empty());
    let fx = r.connect(b, 6_000).unwrap();
    assert_eq!(kinds(&fx), ["participant_updated"]);
    assert!(r.participant(b).unwrap().connected);

    // Disconnect past the grace window: removed.
    r.disconnect(b, 7_000);
    let fx = r.sweep(7_000 + GRACE, GRACE, RESERVE_TTL);
    assert_eq!(kinds(&fx), ["participant_left"]);
    assert!(!r.is_member(b));
    assert!(r.is_member(a));
}

#[test]
fn multiple_tabs_count_connections() {
    let mut r = room(4);
    let a = join(&mut r, "A", false);
    r.connect(a, 0).unwrap();
    assert!(r.disconnect(a, 1).is_empty(), "one tab still open");
    assert_eq!(kinds(&r.disconnect(a, 2)), ["participant_updated"]);
}

#[test]
fn host_only_playback_is_enforced() {
    let mut r = room(4);
    let host = join(&mut r, "Host", false);
    let guest = join(&mut r, "Guest", false);
    r.queue_add(guest, item(1, guest), 0).unwrap();
    assert_eq!(r.playback.status, PlayStatus::Playing, "first video starts immediately");

    let err = r.playback_control(guest, PlaybackAction::Pause, 3.0, 3_000).unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
    let fx = r.playback_control(host, PlaybackAction::Pause, 3.0, 3_000).unwrap();
    assert_eq!(kinds(&fx), ["playback_changed"]);
    assert_eq!(r.playback.status, PlayStatus::Paused);

    r.update_settings(host, None, Some(ControlMode::Everyone), None, None, None)
        .unwrap();
    r.playback_control(guest, PlaybackAction::Seek, 30.0, 4_000).unwrap();
    assert_eq!(r.playback.position, 30.0);
    assert!(matches!(
        r.update_settings(guest, Some("x".into()), None, None, None, None),
        Err(AppError::Forbidden(_))
    ));
}

#[test]
fn invalid_positions_are_rejected() {
    let mut r = room(4);
    let host = join(&mut r, "Host", false);
    assert!(matches!(
        r.playback_control(host, PlaybackAction::Play, 0.0, 0),
        Err(AppError::Conflict(_))
    ));
    r.queue_add(host, item(1, host), 0).unwrap();
    for bad in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(r.playback_control(host, PlaybackAction::Seek, bad, 0).is_err());
    }
}

#[test]
fn queue_flow_and_permissions() {
    let mut r = room(4);
    let host = join(&mut r, "Host", false);
    let guest = join(&mut r, "Guest", false);
    r.queue_add(host, item(1, host), 0).unwrap(); // starts playing
    assert_eq!(kinds(&r.queue_add(guest, item(2, guest), 0).unwrap()), ["queue_updated"]);
    r.queue_add(host, item(3, host), 0).unwrap();
    assert_eq!(r.queue.len(), 2);

    // Guests may remove their own items only.
    assert!(matches!(r.queue_remove(guest, item(3, host).id), Err(AppError::Forbidden(_))));
    r.queue_remove(guest, item(2, guest).id).unwrap();
    assert!(matches!(r.queue_remove(guest, item(2, guest).id), Err(AppError::Conflict(_))));

    // Skipping loads the next one and keeps playing.
    assert!(matches!(r.queue_skip(guest, 0), Err(AppError::Forbidden(_))));
    let fx = r.queue_skip(host, 1_000).unwrap();
    assert_eq!(kinds(&fx), ["queue_updated", "playback_changed"]);
    assert_eq!(r.playback.item.as_ref().unwrap().title, "Video 3");
    // Skipping with an empty queue ends playback.
    r.queue_skip(host, 2_000).unwrap();
    assert_eq!(r.playback.status, PlayStatus::Ended);
    // Adding after the end starts the new video right away.
    r.queue_add(guest, item(4, guest), 3_000).unwrap();
    assert_eq!(r.playback.status, PlayStatus::Playing);

    r.update_settings(host, None, None, Some(QueueMode::HostOnly), None, None).unwrap();
    assert!(matches!(r.queue_add(guest, item(5, guest), 0), Err(AppError::Forbidden(_))));
}

#[test]
fn queue_capacity() {
    let mut r = room(4);
    let host = join(&mut r, "Host", false);
    r.queue_add(host, item(1, host), 0).unwrap();
    for n in 2..=4 {
        r.queue_add(host, item(n, host), 0).unwrap();
    }
    assert!(matches!(r.queue_add(host, item(9, host), 0), Err(AppError::Conflict(_))));
}

#[test]
fn ended_reports_are_validated() {
    let mut r = room(4);
    let host = join(&mut r, "Host", false);
    let guest = join(&mut r, "Guest", false);
    r.queue_add(host, item(1, host), 0).unwrap();
    r.queue_add(host, item(2, host), 0).unwrap();
    let current = item(1, host).id;

    // A viewer cannot end a video early, nor fake a short duration.
    assert!(r.playback_ended(guest, current, 5_000).unwrap().is_empty());
    r.playback_duration(guest, current, 1.0).unwrap();
    assert_eq!(r.playback.duration, None, "viewer reports are ignored");
    assert!(r.playback_ended(guest, current, 5_000).unwrap().is_empty());
    r.playback_duration(host, current, 60.0).unwrap();
    // ...but can once the server agrees it is over.
    let fx = r.playback_ended(guest, current, 58_000).unwrap();
    assert_eq!(kinds(&fx), ["queue_updated", "playback_changed"]);
    // Duplicate/stale report for the old item is ignored.
    assert!(r.playback_ended(host, current, 59_000).unwrap().is_empty());
}

#[test]
fn chat_validation_and_history_ring() {
    let mut r = room(4);
    let a = join(&mut r, "A", false);
    assert!(r.chat(a, "   ", None, None, 0).is_err());
    assert!(r.chat(a, &"x".repeat(501), None, None, 0).is_err());
    let fx = r.chat(a, "hello", Some("n1".into()), None, 0).unwrap();
    assert_eq!(kinds(&fx), ["chat_message_created", "persist_chat"]);
    for n in 0..5 {
        r.chat(a, &format!("m{n}"), None, None, n).unwrap();
    }
    let history = r.chat_history();
    assert_eq!(history.len(), 3);
    assert_eq!(history.last().unwrap().body, "m4");
    assert!(history.iter().all(|m| m.nonce.is_none()));
}

#[test]
fn voice_limits_and_signaling() {
    let mut r = room(4);
    let a = join(&mut r, "A", false);
    let b = join(&mut r, "B", false);
    let c = join(&mut r, "C", false);
    let on = VoiceState {
        muted: false,
        camera: false,
    };
    let offer = serde_json::json!({"kind": "description", "description": {"type": "offer", "sdp": "v=0"}});

    assert!(r.voice_signal(a, b, offer.clone()).is_err(), "must join first");
    r.voice_join(a, on).unwrap();
    assert!(r.voice_signal(a, b, offer.clone()).is_err(), "target must be in call");
    r.voice_join(b, on).unwrap();
    assert!(matches!(r.voice_join(c, on), Err(AppError::Conflict(_))), "max_voice = 2");

    let fx = r.voice_signal(a, b, offer).unwrap();
    assert!(matches!(&fx[0], Effect::To(to, ServerEvent::VoiceSignal { from, .. }) if *to == b && *from == a));
    assert!(r.voice_signal(a, a, serde_json::json!({"kind": "candidate"})).is_err());
    assert!(r.voice_signal(a, b, serde_json::json!({"kind": "evil"})).is_err());
    let huge = serde_json::json!({"kind": "candidate", "x": "a".repeat(30_000)});
    assert!(r.voice_signal(a, b, huge).is_err());

    assert!(r.voice_update(a, on).unwrap().is_empty(), "no-op updates are not broadcast");
    r.voice_update(a, VoiceState { muted: true, camera: true }).unwrap();
    assert_eq!(r.participant(a).unwrap().voice, Some(VoiceState { muted: true, camera: true }));
    r.voice_leave(b).unwrap();
    r.voice_join(c, on).unwrap();
}

#[test]
fn host_transfer() {
    let mut r = room(4);
    let a = join(&mut r, "A", false);
    let b = join(&mut r, "B", false);
    assert!(r.host_transfer(b, a).is_err());
    r.host_transfer(a, b).unwrap();
    assert_eq!(r.host_id(), Some(b));
    assert_eq!(r.participants().iter().filter(|p| p.is_host).count(), 1);
}

#[test]
fn host_kicks_and_voice_seat_frees_on_disconnect() {
    let mut r = room(4);
    let host = join(&mut r, "Host", false);
    let guest = join(&mut r, "Guest", false);
    let other = join(&mut r, "Other", false);
    assert!(matches!(r.kick(guest, other), Err(AppError::Forbidden(_))));
    assert!(r.kick(host, host).is_err());
    let fx = r.kick(host, guest).unwrap();
    assert_eq!(kinds(&fx), ["participant_left"]);
    assert!(!r.is_member(guest));
    assert!(matches!(r.kick(host, guest), Err(AppError::Conflict(_))));

    r.voice_join(
        other,
        VoiceState {
            muted: false,
            camera: false,
        },
    )
    .unwrap();
    r.disconnect(other, 10);
    assert_eq!(r.participant(other).unwrap().voice, None, "a dropped connection leaves the call");
}

#[test]
fn reactions_are_limited_to_the_set() {
    let mut r = room(4);
    let a = join(&mut r, "A", true);
    assert_eq!(kinds(&r.react(a, "🔥").unwrap()), ["reaction"]);
    assert!(r.react(a, "<script>").is_err());
    assert!(matches!(r.react(Uuid::new_v4(), "🔥"), Err(AppError::Unauthorized)));
}

#[test]
fn majority_vote_skips_the_video() {
    let mut r = room(8);
    let host = join(&mut r, "Host", true);
    let b = join(&mut r, "B", false);
    let c = join(&mut r, "C", false);
    let _d = join(&mut r, "D", false);
    r.queue_add(host, item(1, host), 0).unwrap();
    r.queue_add(host, item(2, host), 0).unwrap();
    let current = item(1, host).id;
    assert_eq!(r.skip_votes_needed(), 3, "majority of four");

    assert_eq!(kinds(&r.skip_vote(b, current, true, 1).unwrap()), ["skip_votes"]);
    assert!(r.skip_vote(b, current, true, 1).unwrap().is_empty(), "voting twice changes nothing");
    r.skip_vote(c, current, true, 2).unwrap();
    // A vote can be taken back.
    r.skip_vote(c, current, false, 3).unwrap();
    r.skip_vote(c, current, true, 4).unwrap();
    let fx = r.skip_vote(host, current, true, 5).unwrap();
    assert_eq!(kinds(&fx), ["skip_votes", "queue_updated", "playback_changed"]);
    assert_eq!(r.playback.item.as_ref().unwrap().id, item(2, host).id);
    // Votes for the old video no longer count.
    assert!(r.skip_vote(b, current, true, 6).unwrap().is_empty());
}

#[test]
fn people_who_left_stop_counting_toward_skips() {
    let mut r = room(8);
    let host = join(&mut r, "Host", true);
    let b = join(&mut r, "B", false);
    let c = join(&mut r, "C", false);
    r.queue_add(host, item(1, host), 0).unwrap();
    let current = item(1, host).id;
    r.skip_vote(b, current, true, 1).unwrap();
    r.leave(b);
    // Two people now: two votes needed, so C alone does not skip.
    let fx = r.skip_vote(c, current, true, 2).unwrap();
    assert_eq!(kinds(&fx), ["skip_votes"]);
}

#[test]
fn replies_quote_the_original() {
    let mut r = room(4);
    let a = join(&mut r, "Nita", true);
    let b = join(&mut r, "Riky", false);
    r.chat(a, &"x".repeat(200), None, None, 0).unwrap();
    let original = r.chat_history()[0].id;
    let fx = r.chat(b, "same", None, Some(original), 1).unwrap();
    let Effect::Broadcast(ServerEvent::ChatMessageCreated { message }) = &fx[0] else {
        panic!("expected a chat message");
    };
    let quote = message.reply_to.as_ref().unwrap();
    assert_eq!(quote.display_name, "Nita");
    assert_eq!(quote.body.chars().count(), 141, "excerpt plus ellipsis");
    // An unknown id just drops the quote.
    let fx = r.chat(b, "hm", None, Some(Uuid::new_v4()), 2).unwrap();
    let Effect::Broadcast(ServerEvent::ChatMessageCreated { message }) = &fx[0] else {
        panic!("expected a chat message");
    };
    assert!(message.reply_to.is_none());
}

#[test]
fn host_controls_move_on_when_the_host_is_away_and_come_back() {
    let mut r = room(4);
    let host = join(&mut r, "Host", true);
    let b = join(&mut r, "B", false);
    r.disconnect(host, 1_000);
    assert!(r.sweep(5_000, 60_000, RESERVE_TTL).is_empty(), "too early");
    r.sweep(11_000, 60_000, RESERVE_TTL);
    assert_eq!(r.host_id(), Some(b));
    // The original host returns and gets host controls back.
    r.connect(host, 12_000).unwrap();
    assert_eq!(r.host_id(), Some(host));
}

#[test]
fn deliberate_hand_over_is_not_undone() {
    let mut r = room(4);
    let host = join(&mut r, "Host", true);
    let b = join(&mut r, "B", false);
    let c = join(&mut r, "C", false);
    r.disconnect(host, 0);
    r.sweep(11_000, 60_000, RESERVE_TTL);
    let temp = r.host_id().unwrap();
    assert_ne!(temp, host);
    r.host_transfer(temp, if temp == b { c } else { b }).unwrap();
    r.connect(host, 12_000).unwrap();
    assert_ne!(r.host_id(), Some(host));
}

#[test]
fn new_host_is_someone_still_connected() {
    let mut r = room(4);
    let host = join(&mut r, "Host", true);
    let away = join(&mut r, "Away", false);
    let here = join(&mut r, "Here", false);
    r.disconnect(away, 0);
    r.leave(host);
    assert_eq!(r.host_id(), Some(here));
}

#[test]
fn state_survives_a_save_and_restore() {
    let mut r = room(4);
    let host = join(&mut r, "Host", true);
    r.queue_add(host, item(1, host), 0).unwrap();
    r.queue_add(host, item(2, host), 0).unwrap();
    r.playback_control(host, PlaybackAction::Seek, 30.0, 1_000).unwrap();
    let json = serde_json::to_string(&r.save(1_000)).unwrap();

    // Back quickly: still playing, as if no time was lost.
    let mut quick = room(4);
    quick.restore(serde_json::from_str(&json).unwrap(), 11_000);
    assert_eq!(quick.playback.status, PlayStatus::Playing);
    assert_eq!(quick.playback.position_at(11_000), 40.0);
    assert_eq!(quick.queue.len(), 1);

    // Back much later: paused where everyone stopped.
    let mut late = room(4);
    late.restore(serde_json::from_str(&json).unwrap(), 1_000 + 3_600_000);
    assert_eq!(late.playback.status, PlayStatus::Paused);
    assert_eq!(late.playback.position, 30.0);
}

#[test]
fn playlists_fill_the_queue_up_to_capacity() {
    let mut r = room(4); // queue capacity 3
    let host = join(&mut r, "Host", true);
    let items = (1..=10).map(|n| item(n, host)).collect();
    let (fx, added) = r.queue_add_many(host, items, 0).unwrap();
    assert_eq!(added, 4, "one plays now, three wait");
    assert_eq!(kinds(&fx), ["playback_changed", "queue_updated"]);
    assert_eq!(r.queue.len(), 3);
}

#[test]
fn permanent_is_a_host_setting() {
    let mut r = room(4);
    let host = join(&mut r, "Host", true);
    let guest = join(&mut r, "Guest", false);
    assert!(r.update_settings(guest, None, None, None, Some(true), None).is_err());
    r.update_settings(host, None, None, None, Some(true), None).unwrap();
    assert!(r.info().settings.permanent);
}

#[test]
fn countdown_pauses_then_plays_from_the_same_spot() {
    let mut r = room(4);
    let host = join(&mut r, "Host", true);
    let guest = join(&mut r, "Guest", false);
    assert!(r.start_countdown(host, 0).is_err(), "nothing to play");
    r.queue_add(host, item(1, host), 0).unwrap();
    assert!(r.start_countdown(guest, 1_000).is_err(), "host-only room");

    let (fx, version) = r.start_countdown(host, 10_000).unwrap();
    assert_eq!(kinds(&fx), ["playback_changed", "other"]);
    assert_eq!(r.playback.status, PlayStatus::Paused);
    assert_eq!(r.playback.position, 10.0);
    let fx = r.finish_countdown(version, 13_000);
    assert_eq!(kinds(&fx), ["playback_changed"]);
    assert_eq!(r.playback.status, PlayStatus::Playing);
    assert_eq!(r.playback.position_at(14_000), 11.0);
}

#[test]
fn a_change_during_the_countdown_cancels_it() {
    let mut r = room(4);
    let host = join(&mut r, "Host", true);
    r.queue_add(host, item(1, host), 0).unwrap();
    let (_, version) = r.start_countdown(host, 5_000).unwrap();
    r.playback_control(host, PlaybackAction::Seek, 60.0, 6_000).unwrap();
    assert!(r.finish_countdown(version, 8_000).is_empty());
    assert_eq!(r.playback.status, PlayStatus::Paused);
    assert_eq!(r.playback.position, 60.0);
}
