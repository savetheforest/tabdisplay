//! Crash and error reporting through Sentry. Off unless a DSN is set: at build time in
//! `TABDISPLAY_SENTRY_DSN` (release builds) or, for testing, in the environment when the app starts.
//! Nothing personal goes out: no IPs or usernames (`send_default_pii` stays off), no PC/tablet names,
//! no pairing tokens or codes; events carry only messages written here plus the stack.
use sentry::{Breadcrumb, ClientInitGuard, ClientOptions, Level};
use std::sync::Arc;
use std::time::Duration;

/// Keep the guard alive for the whole run: dropping it flushes pending events.
pub fn init() -> Option<ClientInitGuard> {
    let dsn = option_env!("TABDISPLAY_SENTRY_DSN")
        .map(String::from)
        .or_else(|| std::env::var("TABDISPLAY_SENTRY_DSN").ok())
        .filter(|d| !d.is_empty())?;
    let guard = sentry::init((
        dsn,
        ClientOptions {
            release: sentry::release_name!(),
            send_default_pii: false,
            attach_stacktrace: true,
            before_send: Some(Arc::new(|mut event| {
                event.server_name = None; // the PC's host name
                event.user = None;
                Some(event)
            })),
            ..Default::default()
        },
    ));
    // Release builds abort on panic (Cargo.toml): the Sentry hook queues the event, so wait for it to be sent.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        if let Some(client) = sentry::Hub::current().client() {
            client.flush(Some(Duration::from_secs(2)));
        }
    }));
    Some(guard)
}

/// Something went wrong that we can recover from (capture lost, encoder fell back...): log it and report it.
pub fn warn(message: String) {
    eprintln!("{message}");
    sentry::capture_message(&message, Level::Warning);
}

/// Context for whatever gets reported next (connection steps and the like). Keep it free of names and addresses.
pub fn crumb(category: &str, message: &str) {
    sentry::add_breadcrumb(Breadcrumb { category: Some(category.into()), message: Some(message.into()), ..Default::default() });
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    /// A panic on a background thread reaches a (local, fake) Sentry with its message, and nothing personal.
    #[test]
    fn panics_are_reported() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::env::set_var("TABDISPLAY_SENTRY_DSN", format!("http://key@127.0.0.1:{port}/1"));
        let server = std::thread::spawn(move || {
            listener.set_nonblocking(false).unwrap();
            let (mut sock, _) = listener.accept().unwrap();
            sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut body = Vec::new();
            let mut buf = [0u8; 8192];
            while let Ok(n) = sock.read(&mut buf) {
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&buf[..n]);
                if String::from_utf8_lossy(&body).contains("boom from a background thread") {
                    break;
                }
            }
            let _ = sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}");
            String::from_utf8_lossy(&body).to_string()
        });
        let _guard = super::init().expect("DSN is set");
        let _ = std::thread::spawn(|| panic!("boom from a background thread")).join();
        let received = server.join().unwrap();
        assert!(received.contains("boom from a background thread"), "{received}");
        assert!(!received.contains("server_name"), "host name must not be sent");
    }
}
