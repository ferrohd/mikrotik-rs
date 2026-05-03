//! Integration tests for the MikrotikDevice async client.
//!
//! These tests use a mock router — a local TCP listener that speaks the
//! MikroTik wire protocol. The mock reads commands, verifies them, and
//! sends canned responses, exercising the full async pipeline.

use mikrotik_proto::codec;
use mikrotik_proto::command::CommandBuilder;
use mikrotik_proto::connection::Event;
use mikrotik_tokio::MikrotikDevice;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

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

/// A mock router stream that buffers TCP reads and decodes sentences.
struct MockStream {
    inner: tokio::net::tcp::OwnedWriteHalf,
    reader: tokio::net::tcp::OwnedReadHalf,
    buf: Vec<u8>,
}

impl MockStream {
    fn new(stream: tokio::net::TcpStream) -> Self {
        let (reader, inner) = stream.into_split();
        Self {
            inner,
            reader,
            buf: Vec::new(),
        }
    }

    /// Read one complete sentence from the stream. Returns the decoded words.
    async fn read_sentence(&mut self) -> Vec<String> {
        let mut read_buf = [0u8; 4096];
        loop {
            // First, try to decode from what we already have buffered
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

            // Need more data
            let n = self.reader.read(&mut read_buf).await.expect("read failed");
            if n == 0 {
                panic!("connection closed before sentence complete");
            }
            self.buf.extend_from_slice(&read_buf[..n]);
        }
    }

    /// Write raw bytes to the stream.
    async fn write_all(&mut self, data: &[u8]) {
        self.inner.write_all(data).await.unwrap();
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

// ── Tests ──

#[tokio::test]
async fn connect_and_login() {
    let (listener, addr) = mock_listener().await;

    // Spawn mock router
    let mock = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut mock = MockStream::new(stream);

        // Read the login command
        let words = mock.read_sentence().await;
        assert_eq!(words[0], "/login");
        let tag = extract_tag(&words);

        // Send !done
        mock.write_all(&encode_done(&tag)).await;
    });

    // Connect the client
    let _device = MikrotikDevice::connect(&addr, "admin", Some("password"))
        .await
        .unwrap();

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

    let device = MikrotikDevice::connect(&addr, "admin", Some("password"))
        .await
        .unwrap();

    let cmd = CommandBuilder::new().command("/interface/print").build();
    let mut rx = device.send_command(cmd).await.unwrap();

    // First event: Reply
    let event = rx.recv().await.expect("should receive reply");
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
    let event = rx.recv().await.expect("should receive done");
    assert!(matches!(event, Event::Done { .. }));

    // No more events
    assert!(rx.recv().await.is_none());

    mock.await.unwrap();
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

    let result = MikrotikDevice::connect(&addr, "admin", Some("wrong")).await;
    assert!(result.is_err(), "should fail with auth error");

    mock.await.unwrap();
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

        // Read the command
        let _words = mock.read_sentence().await;

        // Close the connection without responding
        drop(mock);
    });

    let device = MikrotikDevice::connect(&addr, "admin", Some("password"))
        .await
        .unwrap();

    let cmd = CommandBuilder::new().command("/test").build();
    let mut rx = device.send_command(cmd).await.unwrap();

    // The receiver should close (return None) when connection drops.
    // The actor sees Ok(0) from read → sets shutdown → drops response_map → Senders drop.
    while let Some(_event) = rx.recv().await {
        // drain any events before channel close
    }

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

        // Read 3 commands and respond to each
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

    let device = MikrotikDevice::connect(&addr, "admin", Some("pass"))
        .await
        .unwrap();

    // Send 3 commands
    let cmd1 = CommandBuilder::new().command("/test1").build();
    let cmd2 = CommandBuilder::new().command("/test2").build();
    let cmd3 = CommandBuilder::new().command("/test3").build();
    let mut rx1 = device.send_command(cmd1).await.unwrap();
    let mut rx2 = device.send_command(cmd2).await.unwrap();
    let mut rx3 = device.send_command(cmd3).await.unwrap();

    // Each receiver should get its own reply + done (responses arrive in reverse)
    // But each channel only receives events for its own tag.
    // Collect until we see a terminal event (Done).
    async fn collect_until_done(rx: &mut tokio::sync::mpsc::Receiver<Event>) -> Vec<Event> {
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            let is_terminal = matches!(&event, Event::Done { .. } | Event::Empty { .. });
            events.push(event);
            if is_terminal {
                break;
            }
        }
        events
    }

    let (e1, e2, e3) = tokio::join!(
        collect_until_done(&mut rx1),
        collect_until_done(&mut rx2),
        collect_until_done(&mut rx3),
    );

    // Each should have 2 events: Reply + Done
    assert_eq!(e1.len(), 2, "cmd1 events: {e1:?}");
    assert_eq!(e2.len(), 2, "cmd2 events: {e2:?}");
    assert_eq!(e3.len(), 2, "cmd3 events: {e3:?}");

    assert!(matches!(&e1[0], Event::Reply { .. }));
    assert!(matches!(&e1[1], Event::Done { .. }));

    mock.await.unwrap();
}

#[tokio::test]
async fn graceful_shutdown_on_drop() {
    let (listener, addr) = mock_listener().await;

    let mock = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut mock = MockStream::new(stream);

        // Handle login
        let words = mock.read_sentence().await;
        let login_tag = extract_tag(&words);
        mock.write_all(&encode_done(&login_tag)).await;

        // Read a command (don't respond)
        let _words = mock.read_sentence().await;

        // Wait for the client to drop and the TCP connection to close.
        // The actor sees cmd_rx closed → cancel_all → flush → shutdown → wr.shutdown().
        // We should see cancel data, then EOF.
        let mut buf = [0u8; 4096];
        loop {
            match mock.reader.read(&mut buf).await {
                Ok(0) => break,    // Connection closed — success
                Ok(_n) => continue, // Got cancel or other data — keep reading
                Err(_) => break,   // Error — connection dropped
            }
        }
    });

    {
        let device = MikrotikDevice::connect(&addr, "admin", Some("pass"))
            .await
            .unwrap();

        let cmd = CommandBuilder::new()
            .command("/tool/torch")
            .attribute("interface", Some("ether1"))
            .build();
        let _rx = device.send_command(cmd).await.unwrap();

        // Drop device and rx — should trigger graceful shutdown
    }

    // mock.await blocks until the mock task sees the connection close.
    // No sleep needed — the actor shuts down deterministically.
    mock.await.unwrap();
}

#[tokio::test]
async fn fatal_error_propagates_to_all_receivers() {
    let (listener, addr) = mock_listener().await;

    let mock = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut mock = MockStream::new(stream);

        // Handle login
        let words = mock.read_sentence().await;
        let login_tag = extract_tag(&words);
        mock.write_all(&encode_done(&login_tag)).await;

        // Read 2 commands
        let _words1 = mock.read_sentence().await;
        let _words2 = mock.read_sentence().await;

        // Send a fatal error — no need to keep the connection alive,
        // the bytes are in the TCP buffer.
        mock.write_all(&encode_fatal("out of memory")).await;
    });

    let device = MikrotikDevice::connect(&addr, "admin", Some("pass"))
        .await
        .unwrap();

    let cmd1 = CommandBuilder::new().command("/test1").build();
    let cmd2 = CommandBuilder::new().command("/test2").build();
    let mut rx1 = device.send_command(cmd1).await.unwrap();
    let mut rx2 = device.send_command(cmd2).await.unwrap();

    // After the actor processes !fatal:
    //   1. route_event drains response_map → sends Fatal to both Senders → drops them
    //   2. Actor sees conn is dead → shutdown = true → exits → final cleanup
    //   3. Both receivers get Some(Fatal) then None (channel closed)
    let mut got_fatal_1 = false;
    while let Some(event) = rx1.recv().await {
        if matches!(event, Event::Fatal { .. }) {
            got_fatal_1 = true;
        }
    }

    let mut got_fatal_2 = false;
    while let Some(event) = rx2.recv().await {
        if matches!(event, Event::Fatal { .. }) {
            got_fatal_2 = true;
        }
    }

    assert!(got_fatal_1, "rx1 should receive Fatal event");
    assert!(got_fatal_2, "rx2 should receive Fatal event");

    mock.await.unwrap();
}
