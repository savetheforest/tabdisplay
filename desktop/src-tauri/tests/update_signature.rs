//! The updater installs only what the release key signed. Runs the real tauri-plugin-updater against a local
//! server (mock app), the way the app does in production.
//! (An integration test rather than a unit test: its executable needs the manifest build.rs gives test targets.)
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::json;
use std::io::{Cursor, Read, Write};
use std::net::TcpListener;
use tauri_plugin_updater::UpdaterExt;

const PAYLOAD: &[u8] = b"pretend this is the new installer";

/// Serves `latest.json` (pointing at itself for the file) and the file, forever on a thread.
fn serve(version: &str, signature_b64: String, file: &'static [u8]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let manifest = json!({
        "version": version,
        "notes": "teste",
        "pub_date": "2026-09-26T00:00:00Z",
        "platforms": { tauri_plugin_updater::target().unwrap(): { "signature": signature_b64, "url": format!("{base}/installer.bin") } },
    })
    .to_string();
    std::thread::spawn(move || {
        for mut sock in listener.incoming().flatten() {
            let mut buf = [0u8; 2048];
            let n = sock.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let body: Vec<u8> = if request.contains("latest.json") { manifest.clone().into_bytes() } else { file.to_vec() };
            let _ = write!(sock, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = sock.write_all(&body);
        }
    });
    format!("{base}/latest.json")
}

/// (public key as the config wants it, signature of `data` as the manifest wants it)
fn sign(data: &[u8]) -> (String, String) {
    let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let signature = minisign::sign(Some(&pair.pk), &pair.sk, Cursor::new(data), Some("trusted"), Some("untrusted")).unwrap();
    (STANDARD.encode(pair.pk.to_box().unwrap().to_string()), STANDARD.encode(signature.to_string()))
}

/// The installer file the server hands out is `served`, its signature is `signature` and the app trusts `pubkey`.
fn download(pubkey: String, signature: String, served: &'static [u8]) -> Result<Vec<u8>, String> {
    let endpoint = serve("99.0.0", signature, served);
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().plugins.0.insert(
        "updater".into(),
        json!({ "pubkey": pubkey, "endpoints": [endpoint], "dangerousInsecureTransportProtocol": true }),
    );
    let app = tauri::test::mock_builder().plugin(tauri_plugin_updater::Builder::new().build()).build(context).unwrap();
    tauri::async_runtime::block_on(async {
        let update = app.handle().updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?.ok_or("no update offered")?;
        update.download(|_, _| {}, || {}).await.map_err(|e| e.to_string())
    })
}

#[test]
fn installs_only_what_the_release_key_signed() {
    let (pubkey, signature) = sign(PAYLOAD);
    assert_eq!(download(pubkey.clone(), signature.clone(), PAYLOAD).unwrap(), PAYLOAD);

    // The file was swapped after signing.
    let tampered = download(pubkey.clone(), signature, b"pretend this is malware..........");
    assert!(tampered.is_err(), "a tampered installer must be rejected");

    // Signed by some other key: rejected too.
    let (_, foreign_signature) = sign(PAYLOAD);
    assert!(download(pubkey, foreign_signature, PAYLOAD).is_err(), "a signature from another key must be rejected");
}
