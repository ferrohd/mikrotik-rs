//! Integration tests for the Embassy async adapter.
//!
//! These tests use a mock router — a local TCP listener that speaks the
//! MikroTik wire protocol. The client side wraps a tokio `TcpStream` with
//! `embedded_io_adapters::FromTokio` to get `embedded_io_async::Read + Write`,
//! which is what `mikrotik_embassy::run()` expects.
//!
//! The `embassy_sync::Channel`s are backed by `CriticalSectionRawMutex`,
//! with the host `critical-section` impl provided by the `std` feature.
//!
//! Channels are `Box::leak`ed to satisfy `tokio::spawn`'s `'static` bound —
//! this is fine for tests (the process exits when the test finishes).

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embedded_io_adapters::tokio_1::FromTokio;

use mikrotik_proto::codec;
use mikrotik_proto::command::{Command, CommandBuilder};
use mikrotik_proto::connection::Event;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use mikrotik_embassy::error::DeviceError;

// ── Wire-format helpers (simulate what a router sends) ──

fn encode_sentence(words: &[&[u8]]) -> Vec<u8> {
    let mut data = Vec::new();
    for word in words {
        codec::encode_word(word, &mut data);
    }
    codec::encode_terminator(&mut data);
    data
}

fn encode_done(tag: &str) -> Vec<u8> {
    let tag_word = format!(".tag={tag}");
    encode_sentence(&[b"!done", tag_word.as_bytes()])
}

fn encode_reply(tag: &str, attrs: &[(&str, &str)]) -> Vec<u8> {
    let tag_word = format!(".tag={tag}");
    let attr_words: Vec<String> = attrs.iter().map(|(k, v)| format!("={k}={v}")).collect();
    let mut words: Vec<&[u8]> = vec![b"!re", tag_word.as_bytes()];
    for attr in &attr_words {
        words.push(attr.as_bytes());
    }
    encode_sentence(&words)
}

fn encode_trap(tag: &str, message: &str) -> Vec<u8> {
    let tag_word = format!(".tag={tag}");
    let msg_word = format!("=message={message}");
    encode_sentence(&[b"!trap", tag_word.as_bytes(), msg_word.as_bytes()])
}

fn encode_fatal(message: &str) -> Vec<u8> {
    encode_sentence(&[b"!fatal", message.as_bytes()])
}

// ── Mock router stream ──

/// A mock router stream that buffers TCP reads and decodes sentences.
struct MockStream {
    writer: tokio::net::tcp::OwnedWriteHalf,
    reader: tokio::net::tcp::OwnedReadHalf,
    buf: Vec<u8>,
}

impl MockStream {
    fn new(stream: tokio::net::TcpStream) -> Self {
        let (reader, writer) = stream.into_split();
        Self {
            writer,
            reader,
            buf: Vec::new(),
        }
    }

    /// Read one complete sentence from the stream. Returns the decoded words.
    async fn read_sentence(&mut self) -> Vec<String> {
        let mut read_buf = [0u8; 4096];
        loop {
            match codec::decode_sentence(&self.buf) {
                Ok(codec::Decode::Complete {
                    value: raw,
                    bytes_consumed,
                }) => {
                    let words = raw
                        .words()
                        .map(|w| String::from_utf8_lossy(w).into_owned())
                        .collect();
                    self.buf.drain(..bytes_consumed);
                    return words;
                }
                Ok(codec::Decode::Incomplete { .. }) => {}
                Err(e) => panic!("decode error: {e}"),
            }

            let n = self.reader.read(&mut read_buf).await.expect("read failed");
            if n == 0 {
                panic!("connection closed before sentence complete");
            }
            self.buf.extend_from_slice(&read_buf[..n]);
        }
    }

    /// Write raw bytes to the stream.
    async fn write_all(&mut self, data: &[u8]) {
        self.writer.write_all(data).await.unwrap();
    }
}

/// Extract the tag from a list of sentence words (finds `.tag=...`).
fn extract_tag(words: &[String]) -> String {
    words
        .iter()
        .find_map(|w| w.strip_prefix(".tag=").map(String::from))
        .expect("no .tag= word found in sentence")
}

/// Bind a TCP listener on a random available port, return (listener, address).
async fn mock_listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    (listener, addr)
}

/// Leak-allocate embassy channels for use with `tokio::spawn` ('static bound).
/// This is fine for tests — the process exits when the test finishes.
fn leaked_channels() -> (
    &'static Channel<CriticalSectionRawMutex, Command, 4>,
    &'static Channel<CriticalSectionRawMutex, Event, 8>,
) {
    let cmd = Box::leak(Box::new(Channel::new()));
    let evt = Box::leak(Box::new(Channel::new()));
    (cmd, evt)
}

// ── Tests ──

#[tokio::test]
async fn login_success() {
    let (listener, addr) = mock_listener().await;

    let mock = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut mock = MockStream::new(stream);

        // Read the login command
        let words = mock.read_sentence().await;
        assert_eq!(words[0], "/login");
        let tag = extract_tag(&words);

        // Send !done (login success)
        mock.write_all(&encode_done(&tag)).await;

        // Verify run() entered the event loop by reading a command
        let words = mock.read_sentence().await;
        assert_eq!(words[0], "/system/resource/print");
        let cmd_tag = extract_tag(&words);

        // Send done to complete the command
        mock.write_all(&encode_done(&cmd_tag)).await;
    });

    let (cmd_channel, evt_channel) = leaked_channels();

    let stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
    let mut transport = FromTokio::new(stream);

    let run_handle = tokio::spawn(async move {
        mikrotik_embassy::run(
            &mut transport,
            "admin",
            Some("password"),
            cmd_channel.receiver(),
            evt_channel.sender(),
        )
        .await
    });

    // Send a command to verify the event loop is running
    let cmd = CommandBuilder::new()
        .command("/system/resource/print")
        .build();
    cmd_channel.sender().send(cmd).await;

    // Read the Done event
    let event = evt_channel.receiver().receive().await;
    assert!(matches!(event, Event::Done { .. }));

    mock.await.unwrap();

    // run() is still looping — abort to clean up
    run_handle.abort();
}

#[tokio::test]
async fn login_failure() {
    let (listener, addr) = mock_listener().await;

    let mock = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut mock = MockStream::new(stream);

        // Read login command
        let words = mock.read_sentence().await;
        let tag = extract_tag(&words);

        // Send trap (auth failure)
        mock.write_all(&encode_trap(&tag, "invalid user name or password"))
            .await;
    });

    let (cmd_channel, evt_channel) = leaked_channels();

    let stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
    let mut transport = FromTokio::new(stream);

    let result = mikrotik_embassy::run(
        &mut transport,
        "admin",
        Some("wrong"),
        cmd_channel.receiver(),
        evt_channel.sender(),
    )
    .await;

    assert!(matches!(result, Err(DeviceError::Login(_))));

    mock.await.unwrap();
}

#[tokio::test]
async fn send_command_receive_reply() {
    let (listener, addr) = mock_listener().await;

    let mock = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut mock = MockStream::new(stream);

        // Handle login
        let words = mock.read_sentence().await;
        let login_tag = extract_tag(&words);
        mock.write_all(&encode_done(&login_tag)).await;

        // Read the actual command
        let words = mock.read_sentence().await;
        assert_eq!(words[0], "/interface/print");
        let cmd_tag = extract_tag(&words);

        // Send reply + done
        let mut response = encode_reply(&cmd_tag, &[("name", "ether1"), ("type", "ether")]);
        response.extend_from_slice(&encode_done(&cmd_tag));
        mock.write_all(&response).await;
    });

    let (cmd_channel, evt_channel) = leaked_channels();

    let stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
    let mut transport = FromTokio::new(stream);

    let run_handle = tokio::spawn(async move {
        mikrotik_embassy::run(
            &mut transport,
            "admin",
            Some("password"),
            cmd_channel.receiver(),
            evt_channel.sender(),
        )
        .await
    });

    // Send command
    let cmd = CommandBuilder::new().command("/interface/print").build();
    cmd_channel.sender().send(cmd).await;

    // First event: Reply
    let event = evt_channel.receiver().receive().await;
    match event {
        Event::Reply { response, .. } => {
            assert_eq!(
                response.attributes.get("name").unwrap(),
                &Some("ether1".to_string())
            );
            assert_eq!(
                response.attributes.get("type").unwrap(),
                &Some("ether".to_string())
            );
        }
        other => panic!("expected Reply, got {other:?}"),
    }

    // Second event: Done
    let event = evt_channel.receiver().receive().await;
    assert!(matches!(event, Event::Done { .. }));

    mock.await.unwrap();
    run_handle.abort();
}

#[tokio::test]
async fn trap_response() {
    let (listener, addr) = mock_listener().await;

    let mock = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut mock = MockStream::new(stream);

        // Handle login
        let words = mock.read_sentence().await;
        let login_tag = extract_tag(&words);
        mock.write_all(&encode_done(&login_tag)).await;

        // Read the command
        let words = mock.read_sentence().await;
        let cmd_tag = extract_tag(&words);

        // Send trap
        mock.write_all(&encode_trap(&cmd_tag, "no such command prefix"))
            .await;
    });

    let (cmd_channel, evt_channel) = leaked_channels();

    let stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
    let mut transport = FromTokio::new(stream);

    let run_handle = tokio::spawn(async move {
        mikrotik_embassy::run(
            &mut transport,
            "admin",
            Some("password"),
            cmd_channel.receiver(),
            evt_channel.sender(),
        )
        .await
    });

    // Send command
    let cmd = CommandBuilder::new().command("/nonexistent").build();
    cmd_channel.sender().send(cmd).await;

    // Should receive Trap event
    let event = evt_channel.receiver().receive().await;
    match event {
        Event::Trap { response, .. } => {
            assert_eq!(response.message, "no such command prefix");
        }
        other => panic!("expected Trap, got {other:?}"),
    }

    mock.await.unwrap();
    run_handle.abort();
}

#[tokio::test]
async fn fatal_error() {
    let (listener, addr) = mock_listener().await;

    let mock = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut mock = MockStream::new(stream);

        // Handle login
        let words = mock.read_sentence().await;
        let login_tag = extract_tag(&words);
        mock.write_all(&encode_done(&login_tag)).await;

        // Read command
        let _words = mock.read_sentence().await;

        // Send fatal error
        mock.write_all(&encode_fatal("out of memory")).await;
    });

    let (cmd_channel, evt_channel) = leaked_channels();

    let stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
    let mut transport = FromTokio::new(stream);

    let run_handle = tokio::spawn(async move {
        mikrotik_embassy::run(
            &mut transport,
            "admin",
            Some("password"),
            cmd_channel.receiver(),
            evt_channel.sender(),
        )
        .await
    });

    // Send a command to trigger the fatal response
    let cmd = CommandBuilder::new().command("/test").build();
    cmd_channel.sender().send(cmd).await;

    // Should receive Fatal event
    let event = evt_channel.receiver().receive().await;
    match event {
        Event::Fatal { reason } => {
            assert_eq!(reason, "out of memory");
        }
        other => panic!("expected Fatal, got {other:?}"),
    }

    mock.await.unwrap();

    // run() should return an error (connection state machine is dead after fatal)
    let result = run_handle.await.unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn connection_closed_by_remote() {
    let (listener, addr) = mock_listener().await;

    let mock = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut mock = MockStream::new(stream);

        // Handle login
        let words = mock.read_sentence().await;
        let login_tag = extract_tag(&words);
        mock.write_all(&encode_done(&login_tag)).await;

        // Close the connection without sending any command responses
        drop(mock);
    });

    let (cmd_channel, evt_channel) = leaked_channels();

    let stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
    let mut transport = FromTokio::new(stream);

    let result = mikrotik_embassy::run(
        &mut transport,
        "admin",
        Some("password"),
        cmd_channel.receiver(),
        evt_channel.sender(),
    )
    .await;

    assert!(
        matches!(result, Err(DeviceError::ConnectionClosed)),
        "expected ConnectionClosed, got {result:?}"
    );

    mock.await.unwrap();
}

#[tokio::test]
async fn concurrent_commands() {
    let (listener, addr) = mock_listener().await;

    let mock = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut mock = MockStream::new(stream);

        // Handle login
        let words = mock.read_sentence().await;
        let login_tag = extract_tag(&words);
        mock.write_all(&encode_done(&login_tag)).await;

        // Read 3 commands
        let mut tags = Vec::new();
        for _ in 0..3 {
            let words = mock.read_sentence().await;
            let tag = extract_tag(&words);
            tags.push(tag);
        }

        // Respond in reverse order to test demultiplexing
        let mut response = Vec::new();
        for (i, tag) in tags.iter().enumerate().rev() {
            let val = format!("result{i}");
            response.extend_from_slice(&encode_reply(tag, &[("value", &val)]));
            response.extend_from_slice(&encode_done(tag));
        }
        mock.write_all(&response).await;
    });

    let (cmd_channel, evt_channel) = leaked_channels();

    let stream = tokio::net::TcpStream::connect(&addr).await.unwrap();
    let mut transport = FromTokio::new(stream);

    let run_handle = tokio::spawn(async move {
        mikrotik_embassy::run(
            &mut transport,
            "admin",
            Some("pass"),
            cmd_channel.receiver(),
            evt_channel.sender(),
        )
        .await
    });

    // Send 3 commands
    let cmd1 = CommandBuilder::new().command("/test1").build();
    let cmd2 = CommandBuilder::new().command("/test2").build();
    let cmd3 = CommandBuilder::new().command("/test3").build();
    let tag1 = cmd1.tag;
    let tag2 = cmd2.tag;
    let tag3 = cmd3.tag;

    cmd_channel.sender().send(cmd1).await;
    cmd_channel.sender().send(cmd2).await;
    cmd_channel.sender().send(cmd3).await;

    // All 6 events arrive on the single EVT channel (3 Reply + 3 Done).
    // Embassy adapter uses a single channel — consumer filters by tag.
    let mut events = Vec::new();
    for _ in 0..6 {
        events.push(evt_channel.receiver().receive().await);
    }

    // Count Reply and Done events per tag
    let replies_for = |tag| {
        events
            .iter()
            .filter(|e| matches!(e, Event::Reply { tag: t, .. } if *t == tag))
            .count()
    };
    let dones_for = |tag| {
        events
            .iter()
            .filter(|e| matches!(e, Event::Done { tag: t } if *t == tag))
            .count()
    };

    assert_eq!(replies_for(tag1), 1, "cmd1 should have 1 reply");
    assert_eq!(replies_for(tag2), 1, "cmd2 should have 1 reply");
    assert_eq!(replies_for(tag3), 1, "cmd3 should have 1 reply");
    assert_eq!(dones_for(tag1), 1, "cmd1 should have 1 done");
    assert_eq!(dones_for(tag2), 1, "cmd2 should have 1 done");
    assert_eq!(dones_for(tag3), 1, "cmd3 should have 1 done");

    mock.await.unwrap();
    run_handle.abort();
}
