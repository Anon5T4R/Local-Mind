//! End-to-end over a real socket: a local HTTP server answers the OpenAI SSE wire format, the
//! shared client (`local_ai::chat_stream_with_key`) sends the request, reads the stream and returns
//! the text, and the map layer turns that text into nodes.
//!
//! This is the test the in-process mock cannot be: it exercises the request line, the headers, the
//! chunked body, the SSE framing and the timeout-aware reader. No keychain and no external network:
//! the key is handed over directly and the server is a `TcpListener` on `127.0.0.1`.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use local_ai::{App, Connection, Msg};

fn app() -> App {
    App { id: "LocalMindmapE2E", url: "https://example.invalid" }
}

fn connection(port: u16) -> Connection {
    Connection {
        id: "e2e".into(),
        provider: "custom".into(),
        label: "Local server".into(),
        base_url: format!("http://127.0.0.1:{port}/v1"),
        model: "test-model".into(),
    }
}

/// Binds a one-shot server: it accepts a single connection, drains the request and lets `respond`
/// write the answer. Returns the port the client points at.
fn serve(respond: impl FnOnce(&mut TcpStream) + Send + 'static) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local port");
    let port = listener.local_addr().expect("local address").port();
    std::thread::spawn(move || {
        if let Ok((mut socket, _)) = listener.accept() {
            let mut request = [0u8; 8192];
            let _ = socket.read(&mut request);
            respond(&mut socket);
            let _ = socket.flush();
            // Half-close and give the client a moment: dropping the socket right after the last
            // write races the client's read and can turn into a reset instead of a clean EOF.
            let _ = socket.shutdown(std::net::Shutdown::Write);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
    port
}

/// One chunk of a chunked HTTP body.
fn chunk(socket: &mut TcpStream, text: &str) {
    let _ = write!(socket, "{:x}\r\n{}\r\n", text.len(), text);
}

/// One SSE event carrying a `choices[0].delta.content` piece.
fn delta_event(socket: &mut TcpStream, piece: &str) {
    let payload = format!("data: {{\"choices\":[{{\"delta\":{{\"content\":{piece:?}}}}}]}}\n\n");
    chunk(socket, &payload);
}

/// A real streamed answer reaches the caller and becomes a map: the request left the process, the
/// SSE body was read chunk by chunk, the text came back, and the tolerant parser built the tree.
#[test]
fn a_streamed_answer_becomes_a_map() {
    let json = r#"{"text":"Sepse","children":[{"text":"Reconhecimento"},{"text":"Conduta"}]}"#;
    let (head, tail) = json.split_at(12);
    let port = serve(move |socket| {
        let _ = socket.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
        );
        delta_event(socket, head);
        delta_event(socket, tail);
        chunk(socket, "data: [DONE]\n\n");
        let _ = socket.write_all(b"0\r\n\r\n");
    });

    let mut pieces = String::new();
    let answer =
        local_ai::chat_stream_with_key(&app(), &connection(port), "", &[Msg::User("Sepse".into())], &mut |piece: &str| pieces.push_str(piece))
            .expect("the streamed answer arrives");

    assert_eq!(pieces, json, "the deltas arrive in order");
    assert_eq!(answer, json, "the joined answer is the model's JSON");

    let request = localmindmap_ai::Request::GenerateMap { topic: "Sepse".into(), depth: 2, breadth: 4 };
    match localmindmap_ai::actions::interpret(&request, &answer).expect("the answer is a map") {
        localmindmap_ai::Outcome::NewMap { title, nodes } => {
            assert_eq!(title, "Sepse");
            assert_eq!(nodes.first().map(|n| n.text.as_str()), Some("Sepse"));
            let children: Vec<&str> = nodes.iter().filter(|n| n.parent_id.as_deref().is_some()).map(|n| n.text.as_str()).collect();
            assert_eq!(children, ["Reconhecimento", "Conduta"]);
        }
        other => panic!("expected a new map, got {other:?}"),
    }
}

/// A provider error is not swallowed: the status is not a success, the body is read and its message
/// reaches the caller, which is what the UI banner shows.
#[test]
fn a_provider_error_reaches_the_caller() {
    let body = r#"{"error":{"message":"invalid api key"}}"#;
    let port = serve(move |socket| {
        let head = format!(
            "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = socket.write_all(head.as_bytes());
    });

    let error = local_ai::chat_stream_with_key(&app(), &connection(port), "bad-key", &[Msg::User("oi".into())], &mut |_: &str| {})
        .expect_err("a 401 is an error");
    assert!(error.contains("invalid api key"), "the provider's reason is kept: {error}");
}

/// A quota error delivered **inside a 200 SSE stream** (the free tier's shape): the error object is
/// found in the event and its message reaches the caller, which the banner shows.
#[test]
fn a_quota_error_inside_a_stream_is_surfaced() {
    let message = "Free usage limit reached. Free models work in OpenCode; subscribe to OpenCode Go to keep going.";
    let payload = format!("data: {{\"error\":{{\"message\":{message:?}}}}}\n\n");
    let port = serve(move |socket| {
        let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n");
        chunk(socket, &payload);
        chunk(socket, "data: [DONE]\n\n");
        let _ = socket.write_all(b"0\r\n\r\n");
    });
    let error = local_ai::chat_stream_with_key(&app(), &connection(port), "", &[Msg::User("oi".into())], &mut |_: &str| {})
        .expect_err("the quota error is an error");
    assert!(error.contains("Free usage limit reached"), "{error}");
}

/// A quota error delivered as **429 with a JSON body**: the status is not a success, the body is
/// read (not replaced by "http status: 429") and its message reaches the caller.
#[test]
fn a_429_with_a_provider_message_is_surfaced() {
    let body = r#"{"error":{"message":"Insufficient balance. Top up at the console."}}"#;
    let port = serve(move |socket| {
        let head = format!(
            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = socket.write_all(head.as_bytes());
    });
    let error = local_ai::chat_stream_with_key(&app(), &connection(port), "k", &[Msg::User("oi".into())], &mut |_: &str| {})
        .expect_err("the 429 is an error");
    assert!(error.contains("Insufficient balance"), "{error}");
}

/// An OpenRouter-style 429: the generic `error.message` is "Provider returned error", but the real
/// reason is in `error.metadata.raw`, with the upstream provider in `metadata.provider_name`. The
/// caller (and the banner) gets that, not the generic line.
#[test]
fn a_429_with_raw_metadata_shows_the_real_reason() {
    let body = r#"{"error":{"message":"Provider returned error","code":429,"metadata":{"raw":"google/gemma-4-31b-it:free is temporarily rate-limited upstream. Please retry shortly.","provider_name":"Google AI Studio"}}}"#;
    let port = serve(move |socket| {
        let head = format!(
            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = socket.write_all(head.as_bytes());
    });
    let error = local_ai::chat_stream_with_key(&app(), &connection(port), "k", &[Msg::User("oi".into())], &mut |_: &str| {})
        .expect_err("the 429 is an error");
    assert!(error.contains("Google AI Studio"), "{error}");
    assert!(error.contains("temporarily rate-limited upstream"), "{error}");
    assert!(!error.contains("Provider returned error"), "{error}");
}

/// Gemini's OpenAI-compatible base has no `/models` route; the list must go to the native
/// `/v1beta/models` with `x-goog-api-key`, and its ids drop the `models/` prefix. This is the route
/// the live service answers 404 on when pointed at `…/openai/models`.
#[test]
fn gemini_models_use_the_native_route_and_key_header() {
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel::<String>();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local port");
    let port = listener.local_addr().expect("local address").port();
    std::thread::spawn(move || {
        if let Ok((mut socket, _)) = listener.accept() {
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            while let Ok(read) = socket.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let _ = tx.send(String::from_utf8_lossy(&request).to_string());
            let body = r#"{"models":[{"name":"models/gemini-2.5-flash","supportedGenerationMethods":["generateContent"]},{"name":"models/gemini-embedding-001","supportedGenerationMethods":["embedContent"]}]}"#;
            let head =
                format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            let _ = socket.write_all(head.as_bytes());
            let _ = socket.flush();
            let _ = socket.shutdown(std::net::Shutdown::Write);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });

    let connection = Connection {
        id: "g".into(),
        provider: "gemini".into(),
        label: "Gemini".into(),
        base_url: format!("http://127.0.0.1:{port}/v1beta/openai"),
        model: String::new(),
    };
    let models = local_ai::list_models_with_key(&app(), &connection, "test-key").expect("the native list");
    assert_eq!(models, ["gemini-2.5-flash"], "only chat models, without the models/ prefix");
    let request = rx.recv_timeout(std::time::Duration::from_secs(2)).expect("the server saw the request");
    assert!(request.starts_with("GET /v1beta/models "), "the native route: {request}");
    assert!(request.to_ascii_lowercase().contains("x-goog-api-key: test-key"), "the Google key header: {request}");
}

/// The request carries the key as a Bearer header: this is the difference between a 401 and a 200
/// on a real provider. The server echoes the request text back so the test can inspect it (the key
/// is a made-up string, never a real one).
#[test]
fn the_key_travels_as_a_bearer_header() {
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel::<String>();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local port");
    let port = listener.local_addr().expect("local address").port();
    std::thread::spawn(move || {
        if let Ok((mut socket, _)) = listener.accept() {
            // Read until the headers are complete: TCP may deliver the request line alone first.
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            while let Ok(read) = socket.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let _ = tx.send(String::from_utf8_lossy(&request).to_string());
            let _ =
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n");
            let payload = "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\n";
            let _ = write!(socket, "{:x}\r\n{}\r\n", payload.len(), payload);
            let _ = socket.write_all(b"0\r\n\r\n");
            let _ = socket.flush();
            let _ = socket.shutdown(std::net::Shutdown::Write);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });

    let answer = local_ai::chat_stream_with_key(&app(), &connection(port), "secret-key", &[Msg::User("oi".into())], &mut |_: &str| {})
        .expect("the call works with a key");
    assert_eq!(answer, "ok");
    let request = rx.recv_timeout(std::time::Duration::from_secs(2)).expect("the server saw the request");
    assert!(request.to_ascii_lowercase().contains("authorization: bearer secret-key"), "the key goes in the header: {request}");
}
