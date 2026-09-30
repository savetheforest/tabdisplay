//! TLS 1.3 for the session socket. The PC makes a self-signed certificate on first run and keeps it; the tablet
//! pins its fingerprint the first time it connects (like the pairing token, trust on first use).
//!
//! `Conn` is a `TcpStream` look-alike that can be cloned: one clone reads on a thread while another writes.
//! rustls has a single connection state, so it sits behind a mutex that is held only to process bytes, never
//! while blocked on the socket.
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection};
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

static CONFIG: OnceLock<Arc<ServerConfig>> = OnceLock::new();

/// Loads `dir/cert.der` + `dir/key.der`, making them if missing.
pub fn init(dir: PathBuf) {
    let (cert, key) = match (
        std::fs::read(dir.join("cert.der")),
        std::fs::read(dir.join("key.der")),
    ) {
        (Ok(c), Ok(k)) => (c, k),
        _ => {
            let made = rcgen::generate_simple_self_signed(vec!["tabdisplay".to_string()])
                .expect("self-signed certificate");
            let (c, k) = (made.cert.der().to_vec(), made.signing_key.serialize_der());
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join("cert.der"), &c);
            let _ = std::fs::write(dir.join("key.der"), &k);
            (c, k)
        }
    };
    let _ = CONFIG.set(config(cert, key));
}

fn config(cert: Vec<u8>, key: Vec<u8>) -> Arc<ServerConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("TLS 1.3")
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(cert)],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
        )
        .expect("certificate and key match");
    Arc::new(config)
}

pub enum Accepted {
    Tls(Conn),
    /// A client that speaks the old plaintext protocol (its first byte is not a TLS handshake).
    Plain(TcpStream),
}

/// Runs the TLS handshake. `Err(UnexpectedEof)` = the client hung up (the tablet's USB probe does).
pub fn accept(sock: TcpStream) -> io::Result<Accepted> {
    sock.set_nodelay(true)?;
    sock.set_read_timeout(Some(Duration::from_secs(10)))?; // a silent client must not block the server
    sock.set_write_timeout(Some(Duration::from_secs(5)))?; // a peer that stopped reading must not pin a worker
    let mut first = [0u8; 1];
    if sock.peek(&mut first)? == 0 {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    if first[0] != 0x16 {
        sock.set_read_timeout(None)?;
        return Ok(Accepted::Plain(sock)); // TLS records start with 0x16 (handshake)
    }
    let config = CONFIG
        .get()
        .cloned()
        .ok_or_else(|| io::Error::other("TLS not initialised"))?;
    let mut tls = ServerConnection::new(config).map_err(io::Error::other)?;
    let mut s = &sock;
    while tls.is_handshaking() {
        tls.complete_io(&mut s)?;
    }
    sock.set_read_timeout(None)?;
    let conn = Conn {
        sock,
        tls: Arc::new(Mutex::new(tls)),
    };
    conn.set_write_timeout(Some(Duration::from_secs(5)))?;
    Ok(Accepted::Tls(conn))
}

pub struct Conn {
    sock: TcpStream,
    tls: Arc<Mutex<ServerConnection>>,
}

impl Conn {
    pub fn try_clone(&self) -> io::Result<Conn> {
        Ok(Conn {
            sock: self.sock.try_clone()?,
            tls: self.tls.clone(),
        })
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.sock.peer_addr()
    }

    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.sock.set_read_timeout(timeout)
    }

    pub fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.sock.set_write_timeout(timeout)
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        // Break a blocked writer/read first. Do not wait for the TLS mutex: the other worker may
        // currently be inside write_tls, and TCP shutdown is the bounded cancellation primitive.
        let result = self.sock.shutdown(how);
        if let Ok(mut tls) = self.tls.try_lock() {
            tls.send_close_notify();
            let mut s = &self.sock;
            let _ = tls.write_tls(&mut s); // best effort; the socket is already being closed
        }
        result
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut raw = [0u8; 16 * 1024];
        loop {
            match self.tls.lock().unwrap().reader().read(buf) {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {} // no plaintext yet: need more bytes
                other => return other,
            }
            // Blocks on the socket without holding the TLS lock, so the writer clone is never stalled by us.
            let n = (&self.sock).read(&mut raw)?;
            if n == 0 {
                return Ok(0);
            }
            let mut tls = self.tls.lock().unwrap();
            let mut bytes = &raw[..n];
            while !bytes.is_empty() {
                tls.read_tls(&mut bytes)?;
                tls.process_new_packets().map_err(io::Error::other)?;
            }
            let mut s = &self.sock;
            while tls.wants_write() {
                tls.write_tls(&mut s)?; // e.g. key updates the peer asked for
            }
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut tls = self.tls.lock().unwrap();
        let n = tls.writer().write(buf)?;
        let mut s = &self.sock;
        while tls.wants_write() {
            tls.write_tls(&mut s)?;
        }
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::Arc;

    /// Full duplex over a real socket: the client (rustls, trusting any certificate) and the server
    /// exchange messages while one side reads on a clone in another thread.
    #[test]
    fn handshake_and_duplex_traffic() {
        let made = rcgen::generate_simple_self_signed(vec!["tabdisplay".to_string()]).unwrap();
        let _ = CONFIG.set(config(
            made.cert.der().to_vec(),
            made.signing_key.serialize_der(),
        ));

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let Accepted::Tls(mut conn) = accept(listener.accept().unwrap().0).unwrap() else {
                panic!("expected TLS")
            };
            let mut reader = conn.try_clone().unwrap();
            let t = std::thread::spawn(move || {
                let mut got = [0u8; 5];
                reader.read_exact(&mut got).unwrap();
                got
            });
            conn.write_all(b"hello from pc").unwrap();
            let big = vec![7u8; 300_000]; // more than one TLS record / the writer's buffer limit
            conn.write_all(&big).unwrap();
            t.join().unwrap()
        });

        let mut client = client_stream(addr);
        let mut greeting = [0u8; 13];
        client.read_exact(&mut greeting).unwrap();
        assert_eq!(&greeting, b"hello from pc");
        let mut big = vec![0u8; 300_000];
        client.read_exact(&mut big).unwrap();
        assert!(big.iter().all(|&b| b == 7));
        client.write_all(b"hi pc").unwrap();
        assert_eq!(&server.join().unwrap(), b"hi pc");
    }

    #[test]
    fn plaintext_client_is_recognised() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let c = std::thread::spawn(move || {
            TcpStream::connect(addr)
                .unwrap()
                .write_all(&[1, 0, 0, 0, 0])
                .unwrap()
        });
        assert!(matches!(
            accept(listener.accept().unwrap().0).unwrap(),
            Accepted::Plain(_)
        ));
        c.join().unwrap();
    }

    #[test]
    fn write_timeout_stops_when_peer_stops_reading() {
        let made = rcgen::generate_simple_self_signed(vec!["tabdisplay".to_string()]).unwrap();
        let _ = CONFIG.set(config(
            made.cert.der().to_vec(),
            made.signing_key.serialize_der(),
        ));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let Accepted::Tls(mut conn) = accept(listener.accept().unwrap().0).unwrap() else {
                panic!("expected TLS")
            };
            conn.set_write_timeout(Some(Duration::from_millis(100)))
                .unwrap();
            let started = std::time::Instant::now();
            let result = conn.write_all(&vec![7u8; 64 * 1024 * 1024]);
            (result, started.elapsed())
        });
        let mut client = client_stream(addr);
        // Writing initiates the TLS handshake; after this the client deliberately
        // does not read the server's records.
        client.write_all(b"client hello").unwrap();
        let (result, elapsed) = server.join().unwrap();
        assert!(
            result.is_err(),
            "a non-reading peer must not accept an unbounded write"
        );
        assert!(
            elapsed < Duration::from_secs(3),
            "write timeout took {elapsed:?}"
        );
    }

    #[test]
    fn reconnect_cycles_do_not_leave_tls_workers_or_sockets_behind() {
        let made = rcgen::generate_simple_self_signed(vec!["tabdisplay".to_string()]).unwrap();
        let _ = CONFIG.set(config(
            made.cert.der().to_vec(),
            made.signing_key.serialize_der(),
        ));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for _ in 0..100 {
                let (socket, _) = listener.accept().unwrap();
                let Accepted::Tls(mut conn) = accept(socket).unwrap() else {
                    return Err(io::Error::other("expected TLS"));
                };
                let mut byte = [0u8; 1];
                conn.read_exact(&mut byte)?;
                if byte != [0x5a] {
                    return Err(io::Error::other("unexpected reconnect payload"));
                }
                conn.write_all(&[0xa5])?;
                conn.shutdown(Shutdown::Both)?;
            }
            Ok::<(), io::Error>(())
        });

        for _ in 0..100 {
            let mut client = client_stream(addr);
            client.write_all(&[0x5a]).unwrap();
            let mut ack = [0u8; 1];
            client.read_exact(&mut ack).unwrap();
            assert_eq!(ack, [0xa5]);
        }
        server.join().unwrap().unwrap();
    }

    fn client_stream(addr: SocketAddr) -> rustls::StreamOwned<rustls::ClientConnection, TcpStream> {
        use rustls::client::danger::{
            HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
        };
        use rustls::pki_types::{ServerName, UnixTime};
        use rustls::{DigitallySignedStruct, SignatureScheme};

        #[derive(Debug)]
        struct Any;
        impl ServerCertVerifier for Any {
            fn verify_server_cert(
                &self,
                _: &CertificateDer<'_>,
                _: &[CertificateDer<'_>],
                _: &ServerName<'_>,
                _: &[u8],
                _: UnixTime,
            ) -> Result<ServerCertVerified, rustls::Error> {
                Ok(ServerCertVerified::assertion())
            }
            fn verify_tls12_signature(
                &self,
                _: &[u8],
                _: &CertificateDer<'_>,
                _: &DigitallySignedStruct,
            ) -> Result<HandshakeSignatureValid, rustls::Error> {
                Ok(HandshakeSignatureValid::assertion())
            }
            fn verify_tls13_signature(
                &self,
                _: &[u8],
                _: &CertificateDer<'_>,
                _: &DigitallySignedStruct,
            ) -> Result<HandshakeSignatureValid, rustls::Error> {
                Ok(HandshakeSignatureValid::assertion())
            }
            fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
                rustls::crypto::ring::default_provider()
                    .signature_verification_algorithms
                    .supported_schemes()
            }
        }
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Any))
        .with_no_client_auth();
        let conn =
            rustls::ClientConnection::new(Arc::new(config), "tabdisplay".try_into().unwrap())
                .unwrap();
        rustls::StreamOwned::new(conn, TcpStream::connect(addr).unwrap())
    }
}
