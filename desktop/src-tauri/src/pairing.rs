//! Wi‑Fi pairing. A tablet this PC doesn't know must type, once, the 6-digit code the PC shows; it then gets
//! a random token it presents in every later HELLO. USB (loopback) connections skip this: the cable is proof.
//! ponytail: tokens stored in plain JSON in the user's app data and sent unencrypted on the LAN; this stops
//! strangers from driving the mouse, not a sniffer on the same network. TLS if that ever matters.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CODE_TTL: Duration = Duration::from_secs(120);
const MAX_ATTEMPTS: u8 = 5;

#[derive(Default, Serialize, Deserialize)]
struct Store {
    pc_id: String,
    devices: Vec<Device>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    token: String,
}

struct Pending {
    device_name: String,
    code: String,
    expires: Instant,
    attempts: u8,
}

static STORE: Mutex<Option<(PathBuf, Store)>> = Mutex::new(None);
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

pub fn init(dir: PathBuf) {
    let path = dir.join("paired.json");
    let mut store: Store = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    if store.pc_id.is_empty() {
        store.pc_id = random_hex(8);
        save(&path, &store);
    }
    *STORE.lock().unwrap() = Some((path, store));
}

fn save(path: &PathBuf, store: &Store) {
    let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) else { return }; // not init'ed (tests)
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(path, serde_json::to_string_pretty(store).unwrap());
}

fn with<T>(f: impl FnOnce(&PathBuf, &mut Store) -> T) -> T {
    let mut guard = STORE.lock().unwrap();
    let (path, store) = guard.get_or_insert_with(|| (PathBuf::new(), Store::default()));
    f(path, store)
}

/// Stable id of this PC, sent in the discovery beacon so a tablet can match its stored token.
pub fn pc_id() -> String {
    with(|_, s| s.pc_id.clone())
}

pub fn devices() -> Vec<Device> {
    with(|_, s| s.devices.clone())
}

pub fn forget(id: &str) {
    with(|path, s| {
        s.devices.retain(|d| d.id != id);
        save(path, s);
    })
}

pub fn is_paired(id: &str, token: &str) -> bool {
    with(|_, s| s.devices.iter().any(|d| d.id == id && !token.is_empty() && d.token == token))
}

/// Shows a fresh code on the PC for `device_name` and returns it.
pub fn start(device_name: &str) -> String {
    let code = format!("{:06}", u32::from_le_bytes(random::<4>()) % 1_000_000);
    *PENDING.lock().unwrap() = Some(Pending { device_name: device_name.into(), code: code.clone(), expires: Instant::now() + CODE_TTL, attempts: 0 });
    code
}

/// The code on screen right now, with who asked for it.
pub fn current() -> Option<(String, String)> {
    let mut p = PENDING.lock().unwrap();
    if p.as_ref().is_some_and(|p| Instant::now() > p.expires) {
        *p = None;
    }
    p.as_ref().map(|p| (p.code.clone(), p.device_name.clone()))
}

pub fn cancel() {
    *PENDING.lock().unwrap() = None;
}

#[derive(Debug, PartialEq)]
pub enum Check {
    Ok,
    Wrong,
    /// Expired or too many attempts: the tablet has to start over.
    Over,
}

pub fn check(code: &str) -> Check {
    let mut guard = PENDING.lock().unwrap();
    let Some(p) = guard.as_mut() else { return Check::Over };
    if Instant::now() > p.expires || p.attempts >= MAX_ATTEMPTS {
        *guard = None;
        return Check::Over;
    }
    if code.trim() == p.code {
        *guard = None;
        return Check::Ok;
    }
    p.attempts += 1;
    if p.attempts >= MAX_ATTEMPTS {
        *guard = None;
        return Check::Over;
    }
    Check::Wrong
}

/// Records the tablet as paired (replacing an older pairing of the same device) and returns its token.
pub fn complete(id: &str, name: &str) -> String {
    let token = random_hex(32);
    with(|path, s| {
        s.devices.retain(|d| d.id != id);
        s.devices.push(Device { id: id.into(), name: name.into(), token: token.clone() });
        save(path, s);
    });
    token
}

fn random<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).expect("OS random generator");
    buf
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("OS random generator");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_flow() {
        let code = start("Redmi Pad 2");
        assert_eq!(code.len(), 6);
        assert_eq!(current().unwrap().1, "Redmi Pad 2");
        let wrong = if code == "000000" { "111111" } else { "000000" };
        assert_eq!(check(wrong), Check::Wrong);
        assert_eq!(check(&code), Check::Ok);
        assert_eq!(check(&code), Check::Over); // single use

        let token = complete("tab-1", "Redmi Pad 2");
        assert!(is_paired("tab-1", &token));
        assert!(!is_paired("tab-1", "nope") && !is_paired("tab-2", &token) && !is_paired("tab-1", ""));
        forget("tab-1");
        assert!(!is_paired("tab-1", &token));

        start("x");
        for _ in 0..MAX_ATTEMPTS - 1 {
            assert_eq!(check("bad"), Check::Wrong);
        }
        assert_eq!(check("bad"), Check::Over); // too many attempts ends it
    }
}
