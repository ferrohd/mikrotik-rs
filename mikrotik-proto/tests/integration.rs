//! Integration tests for mikrotik-proto.
//!
//! These tests drive the full pipeline: command building → connection state machine
//! → wire encoding → decoding → response parsing → event polling.
//! Unlike unit tests, they exercise multiple modules composed together.

use mikrotik_proto::codec;
use mikrotik_proto::command;
use mikrotik_proto::command::CommandBuilder;
use mikrotik_proto::connection::{Connection, Event, State};
use mikrotik_proto::handshake::{Handshaking, LoginProgress};
use mikrotik_proto::response::TrapCategory;
use mikrotik_proto::tag::Tag;

// ── Test helpers ──

/// Build a wire-format sentence from raw word byte slices.
/// This simulates what a MikroTik router would send.
fn build_sentence(words: &[&[u8]]) -> Vec<u8> {
    let mut data = Vec::new();
    for word in words {
        codec::encode_word(word, &mut data);
    }
    codec::encode_terminator(&mut data);
    data
}

fn build_done(tag: Tag) -> Vec<u8> {
    let tag_word = format!(".tag={tag}");
    build_sentence(&[b"!done", tag_word.as_bytes()])
}

fn build_reply(tag: Tag, attrs: &[(&str, &str)]) -> Vec<u8> {
    let tag_word = format!(".tag={tag}");
    let attr_words: Vec<String> = attrs.iter().map(|(k, v)| format!("={k}={v}")).collect();
    let mut words: Vec<&[u8]> = vec![b"!re", tag_word.as_bytes()];
    for attr in &attr_words {
        words.push(attr.as_bytes());
    }
    build_sentence(&words)
}

fn build_trap_with_category(tag: Tag, category: u8, message: &str) -> Vec<u8> {
    let tag_word = format!(".tag={tag}");
    let cat_word = format!("=category={category}");
    let msg_word = format!("=message={message}");
    build_sentence(&[
        b"!trap",
        tag_word.as_bytes(),
        cat_word.as_bytes(),
        msg_word.as_bytes(),
    ])
}

fn build_fatal(message: &str) -> Vec<u8> {
    build_sentence(&[b"!fatal", message.as_bytes()])
}

/// Drain all pending transmits from a connection (discarding the data).
fn drain_transmits(conn: &mut Connection) {
    while conn.poll_transmit().is_some() {}
}

/// Collect all pending events from a connection.
fn drain_events(conn: &mut Connection) -> Vec<Event> {
    let mut events = Vec::new();
    while let Some(event) = conn.poll_event() {
        events.push(event);
    }
    events
}

// ── Full lifecycle tests ──

#[test]
fn full_command_lifecycle() {
    // Build a command using the builder
    let cmd = CommandBuilder::new()
        .command("/interface/print")
        .attribute("detail", None)
        .build();
    let tag = cmd.tag;

    // Send it through the connection
    let mut conn = Connection::new();
    let returned_tag = conn.send_command(cmd).unwrap();
    assert_eq!(returned_tag, tag);
    assert!(conn.is_in_flight(tag));

    // Drain the outbound transmit
    let transmit = conn.poll_transmit().unwrap();
    assert!(!transmit.data.is_empty());
    assert!(conn.poll_transmit().is_none());

    // Simulate router response: one reply row + done
    let reply_wire = build_reply(
        tag,
        &[("name", "ether1"), ("type", "ether"), ("mtu", "1500")],
    );
    let done_wire = build_done(tag);

    let mut response = reply_wire;
    response.extend_from_slice(&done_wire);
    conn.receive(&response).unwrap();

    // Poll events
    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 2);

    // First event: Reply
    match &events[0] {
        Event::Reply { tag: t, response } => {
            assert_eq!(*t, tag);
            assert_eq!(
                response.attributes.get("name").unwrap(),
                &Some("ether1".to_string())
            );
            assert_eq!(
                response.attributes.get("type").unwrap(),
                &Some("ether".to_string())
            );
            assert_eq!(
                response.attributes.get("mtu").unwrap(),
                &Some("1500".to_string())
            );
        }
        other => panic!("expected Reply, got {other:?}"),
    }

    // Second event: Done
    match &events[1] {
        Event::Done { tag: t } => assert_eq!(*t, tag),
        other => panic!("expected Done, got {other:?}"),
    }

    // Command is no longer in-flight
    assert!(!conn.is_in_flight(tag));
    assert_eq!(conn.in_flight_count(), 0);
}

#[test]
fn handshake_then_commands() {
    // Start the login handshake
    let mut hs = Handshaking::new("admin", Some("secret")).unwrap();

    // Drain the login command transmit
    let login_transmit = hs.poll_transmit().unwrap();
    assert!(!login_transmit.data.is_empty());
    assert!(hs.poll_transmit().is_none());

    // Simulate router accepting login
    let done_wire = build_done(hs.login_tag());
    hs.receive(&done_wire).unwrap();

    // Advance to authenticated
    let mut conn = match hs.advance().unwrap() {
        LoginProgress::Complete(auth) => auth.into_connection(),
        LoginProgress::Pending(_) => panic!("expected login to complete"),
    };

    assert!(conn.is_active());
    assert_eq!(conn.in_flight_count(), 0);

    // Now send a real command
    let cmd = CommandBuilder::new()
        .command("/system/resource/print")
        .build();
    let tag = conn.send_command(cmd).unwrap();
    drain_transmits(&mut conn);

    // Simulate router response
    let reply_wire = build_reply(
        tag,
        &[
            ("uptime", "3d12h"),
            ("cpu-load", "5"),
            ("free-memory", "1073741824"),
        ],
    );
    let done_wire = build_done(tag);
    conn.receive(&reply_wire).unwrap();
    conn.receive(&done_wire).unwrap();

    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 2);

    match &events[0] {
        Event::Reply { response, .. } => {
            assert_eq!(
                response.attributes.get("uptime").unwrap(),
                &Some("3d12h".to_string())
            );
        }
        other => panic!("expected Reply, got {other:?}"),
    }
    match &events[1] {
        Event::Done { tag: t } => assert_eq!(*t, tag),
        other => panic!("expected Done, got {other:?}"),
    }
}

#[test]
fn concurrent_multiplexing() {
    let mut conn = Connection::new();

    // Send 3 commands
    let cmd1 = CommandBuilder::new().command("/interface/print").build();
    let cmd2 = CommandBuilder::new().command("/ip/address/print").build();
    let cmd3 = CommandBuilder::new()
        .command("/system/resource/print")
        .build();
    let tag1 = conn.send_command(cmd1).unwrap();
    let tag2 = conn.send_command(cmd2).unwrap();
    let tag3 = conn.send_command(cmd3).unwrap();
    drain_transmits(&mut conn);

    assert_eq!(conn.in_flight_count(), 3);

    // Simulate interleaved responses in a single receive
    let mut wire = Vec::new();
    wire.extend_from_slice(&build_reply(tag2, &[("address", "192.168.1.1/24")]));
    wire.extend_from_slice(&build_reply(tag1, &[("name", "ether1")]));
    wire.extend_from_slice(&build_done(tag2));
    wire.extend_from_slice(&build_reply(tag3, &[("uptime", "1d")]));
    wire.extend_from_slice(&build_done(tag1));
    wire.extend_from_slice(&build_done(tag3));

    conn.receive(&wire).unwrap();

    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 6);

    // Verify correct tag correlation
    // Event 0: Reply for tag2
    match &events[0] {
        Event::Reply { tag, response } => {
            assert_eq!(*tag, tag2);
            assert_eq!(
                response.attributes.get("address").unwrap(),
                &Some("192.168.1.1/24".to_string())
            );
        }
        other => panic!("expected Reply for tag2, got {other:?}"),
    }
    // Event 1: Reply for tag1
    match &events[1] {
        Event::Reply { tag, .. } => assert_eq!(*tag, tag1),
        other => panic!("expected Reply for tag1, got {other:?}"),
    }
    // Event 2: Done for tag2
    match &events[2] {
        Event::Done { tag } => assert_eq!(*tag, tag2),
        other => panic!("expected Done for tag2, got {other:?}"),
    }
    // Event 3: Reply for tag3
    match &events[3] {
        Event::Reply { tag, .. } => assert_eq!(*tag, tag3),
        other => panic!("expected Reply for tag3, got {other:?}"),
    }
    // Event 4: Done for tag1
    match &events[4] {
        Event::Done { tag } => assert_eq!(*tag, tag1),
        other => panic!("expected Done for tag1, got {other:?}"),
    }
    // Event 5: Done for tag3
    match &events[5] {
        Event::Done { tag } => assert_eq!(*tag, tag3),
        other => panic!("expected Done for tag3, got {other:?}"),
    }

    assert_eq!(conn.in_flight_count(), 0);
}

#[test]
fn cancel_mid_stream() {
    let mut conn = Connection::new();
    let cmd = CommandBuilder::new()
        .command("/tool/torch")
        .attribute("interface", Some("ether1"))
        .build();
    let tag = conn.send_command(cmd).unwrap();
    drain_transmits(&mut conn);

    // Simulate two streaming replies
    conn.receive(&build_reply(tag, &[("tx", "1000"), ("rx", "2000")]))
        .unwrap();
    conn.receive(&build_reply(tag, &[("tx", "1100"), ("rx", "2100")]))
        .unwrap();

    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 2);

    // Cancel the command
    conn.cancel_command(tag).unwrap();
    assert!(!conn.is_in_flight(tag));

    // A cancel transmit should be queued
    let cancel_transmit = conn.poll_transmit().unwrap();
    assert!(!cancel_transmit.data.is_empty());

    // Router may still send a done for the cancelled tag — should be silently dropped
    conn.receive(&build_done(tag)).unwrap();
    let events = drain_events(&mut conn);
    // Done for unknown tag should produce a Done event anyway (it goes through dispatch)
    // But the in_flight was already removed, so no crash
    assert!(events.len() <= 1);
}

#[test]
fn command_macro_roundtrip() {
    let cmd = command!("/interface/print", name = "ether1");
    let tag = cmd.tag;

    let mut conn = Connection::new();
    conn.send_command(cmd).unwrap();
    drain_transmits(&mut conn);

    // Feed a matching response
    conn.receive(&build_reply(tag, &[("name", "ether1"), ("type", "ether")]))
        .unwrap();
    conn.receive(&build_done(tag)).unwrap();

    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 2);

    match &events[0] {
        Event::Reply { tag: t, response } => {
            assert_eq!(*t, tag);
            assert!(response.attributes.contains_key("name"));
        }
        other => panic!("expected Reply, got {other:?}"),
    }
}

#[test]
fn byte_at_a_time() {
    let mut conn = Connection::new();
    let cmd = CommandBuilder::new().command("/test").build();
    let tag = conn.send_command(cmd).unwrap();
    drain_transmits(&mut conn);

    let wire = build_done(tag);

    // Feed one byte at a time — must not panic, must eventually produce event
    for &byte in &wire {
        conn.receive(&[byte]).unwrap();
    }

    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::Done { tag: t } => assert_eq!(*t, tag),
        other => panic!("expected Done, got {other:?}"),
    }
}

#[test]
fn back_to_back_streaming_replies() {
    let mut conn = Connection::new();
    let cmd = CommandBuilder::new().command("/interface/print").build();
    let tag = conn.send_command(cmd).unwrap();
    drain_transmits(&mut conn);

    // Build 10 reply sentences + 1 done, all concatenated
    let mut wire = Vec::new();
    for i in 0..10 {
        let name = format!("ether{i}");
        wire.extend_from_slice(&build_reply(tag, &[("name", &name)]));
    }
    wire.extend_from_slice(&build_done(tag));

    conn.receive(&wire).unwrap();

    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 11); // 10 replies + 1 done

    // Verify all 10 replies have correct data
    for i in 0..10 {
        match &events[i] {
            Event::Reply { tag: t, response } => {
                assert_eq!(*t, tag);
                let expected_name = format!("ether{i}");
                assert_eq!(
                    response.attributes.get("name").unwrap(),
                    &Some(expected_name)
                );
            }
            other => panic!("expected Reply at index {i}, got {other:?}"),
        }
    }

    match &events[10] {
        Event::Done { tag: t } => assert_eq!(*t, tag),
        other => panic!("expected Done, got {other:?}"),
    }
}

#[test]
fn trap_with_all_categories() {
    let categories = [
        (0u8, TrapCategory::MissingItemOrCommand),
        (1, TrapCategory::ArgumentValueFailure),
        (2, TrapCategory::CommandExecutionInterrupted),
        (3, TrapCategory::ScriptingFailure),
        (4, TrapCategory::GeneralFailure),
        (5, TrapCategory::APIFailure),
        (6, TrapCategory::TTYFailure),
        (7, TrapCategory::ReturnValue),
    ];

    for (cat_num, expected_cat) in categories {
        let mut conn = Connection::new();
        let cmd = CommandBuilder::new().command("/test").build();
        let tag = conn.send_command(cmd).unwrap();
        drain_transmits(&mut conn);

        let wire = build_trap_with_category(tag, cat_num, "test error");
        conn.receive(&wire).unwrap();

        let events = drain_events(&mut conn);
        assert_eq!(events.len(), 1, "category {cat_num}");
        match &events[0] {
            Event::Trap { tag: t, response } => {
                assert_eq!(*t, tag);
                assert_eq!(response.category, Some(expected_cat));
                assert_eq!(response.message, "test error");
            }
            other => panic!("expected Trap for category {cat_num}, got {other:?}"),
        }
    }
}

#[test]
fn fatal_kills_connection() {
    let mut conn = Connection::new();

    let cmd1 = CommandBuilder::new().command("/test1").build();
    let cmd2 = CommandBuilder::new().command("/test2").build();
    let tag1 = conn.send_command(cmd1).unwrap();
    let _tag2 = conn.send_command(cmd2).unwrap();
    drain_transmits(&mut conn);

    // Send a reply for cmd1 first
    conn.receive(&build_reply(tag1, &[("data", "hello")]))
        .unwrap();

    // Then fatal
    conn.receive(&build_fatal("out of memory")).unwrap();

    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 2); // reply + fatal

    match &events[0] {
        Event::Reply { tag, .. } => assert_eq!(*tag, tag1),
        other => panic!("expected Reply, got {other:?}"),
    }
    match &events[1] {
        Event::Fatal { reason } => assert_eq!(reason, "out of memory"),
        other => panic!("expected Fatal, got {other:?}"),
    }

    // Connection is dead
    assert_eq!(conn.state(), State::Dead);
    assert_eq!(conn.in_flight_count(), 0);

    // Further operations must fail
    let cmd3 = CommandBuilder::new().command("/test3").build();
    assert!(conn.send_command(cmd3).is_err());
    assert!(conn.receive(&[]).is_err());
}

#[test]
fn empty_password_login() {
    // Login with no password
    let mut hs = Handshaking::new("admin", None).unwrap();

    let transmit = hs.poll_transmit().unwrap();
    // The login command should contain =name=admin and =password=
    let data = &transmit.data;

    // Decode the sentence to verify its contents
    match codec::decode_sentence(data).unwrap() {
        codec::Decode::Complete { value: raw, .. } => {
            let words: Vec<&[u8]> = raw.words().collect();
            assert_eq!(words[0], b"/login");
            // Find the password word
            let password_word = words
                .iter()
                .find(|w| w.starts_with(b"=password="))
                .expect("should have password attribute");
            assert_eq!(*password_word, b"=password=");
        }
        codec::Decode::Incomplete { .. } => panic!("expected complete sentence"),
    }

    // Complete the login
    let done_wire = build_done(hs.login_tag());
    hs.receive(&done_wire).unwrap();
    match hs.advance().unwrap() {
        LoginProgress::Complete(_) => {}
        LoginProgress::Pending(_) => panic!("expected login to complete"),
    }
}

#[test]
fn reply_for_cancelled_command_is_ignored() {
    let mut conn = Connection::new();
    let cmd = CommandBuilder::new().command("/tool/torch").build();
    let tag = conn.send_command(cmd).unwrap();
    drain_transmits(&mut conn);

    // Cancel immediately
    conn.cancel_command(tag).unwrap();
    drain_transmits(&mut conn);

    // Router sends replies after cancellation
    conn.receive(&build_reply(tag, &[("tx", "500")])).unwrap();
    conn.receive(&build_done(tag)).unwrap();

    let events = drain_events(&mut conn);
    // Replies for unknown tags are silently dropped; Done still produces an event
    // because dispatch_response always pushes Done regardless of in_flight status
    for event in &events {
        match event {
            Event::Reply { tag: t, .. } => {
                panic!("should not receive reply for cancelled tag {t:?}")
            }
            _ => {} // Done is acceptable
        }
    }
}

#[test]
fn multiple_receive_chunks_split_across_sentence_boundary() {
    let mut conn = Connection::new();
    let cmd = CommandBuilder::new().command("/test").build();
    let tag = conn.send_command(cmd).unwrap();
    drain_transmits(&mut conn);

    let reply = build_reply(tag, &[("key", "value")]);
    let done = build_done(tag);

    // Concatenate and split at an arbitrary point within the reply
    let mut combined = reply.clone();
    combined.extend_from_slice(&done);

    let split_point = reply.len() / 2; // mid-reply
    conn.receive(&combined[..split_point]).unwrap();

    // Should have no events yet (sentence incomplete)
    assert!(conn.poll_event().is_none());

    conn.receive(&combined[split_point..]).unwrap();

    let events = drain_events(&mut conn);
    assert_eq!(events.len(), 2);
    assert!(matches!(&events[0], Event::Reply { .. }));
    assert!(matches!(&events[1], Event::Done { .. }));
}
