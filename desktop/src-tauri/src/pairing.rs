//! Wi‑Fi pairing. A tablet this PC doesn't know must type, once, the 6-digit code the PC shows; it then gets
//! a random token it presents in every later HELLO. Loopback is only an endpoint, not proof of a particular cable.
//! Tokens are protected in transit by TLS; storage permissions and backup policy remain deployment concerns.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CODE_TTL: Duration = Duration::from_secs(120);
const MAX_ATTEMPTS: u8 = 5;

#[derive(Clone, Default, Serialize, Deserialize)]
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
    let mut store: Store = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if store.pc_id.is_empty() {
        store.pc_id = random_hex(8);
        let _ = save(&path, &store);
    }
    *STORE.lock().unwrap() = Some((path, store));
}

fn save(path: &PathBuf, store: &Store) -> Result<(), String> {
    if path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .is_none()
    {
        return Ok(()); // not init'ed (tests)
    }
    let text = serde_json::to_vec_pretty(store)
        .map_err(|e| format!("não foi possível serializar pareamento: {e}"))?;
    crate::settings::persist(path, &text)
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

fn forget_store(path: &PathBuf, store: &mut Store, id: &str) -> Result<(), String> {
    let mut next = store.clone();
    next.devices.retain(|d| d.id != id);
    save(path, &next)?;
    *store = next;
    Ok(())
}

pub fn forget(id: &str) -> Result<(), String> {
    let mut guard = STORE.lock().unwrap();
    let (path, store) = guard.get_or_insert_with(|| (PathBuf::new(), Store::default()));
    forget_store(path, store, id)
}

pub fn is_paired(id: &str, token: &str) -> bool {
    with(|_, s| {
        s.devices
            .iter()
            .any(|d| d.id == id && !token.is_empty() && d.token == token)
    })
}

/// Shows a fresh code on the PC for `device_name` and returns it.
pub fn start(device_name: &str) -> String {
    let code = format!("{:06}", u32::from_le_bytes(random::<4>()) % 1_000_000);
    *PENDING.lock().unwrap() = Some(Pending {
        device_name: device_name.into(),
        code: code.clone(),
        expires: Instant::now() + CODE_TTL,
        attempts: 0,
    });
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
    let Some(p) = guard.as_mut() else {
        return Check::Over;
    };
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
fn complete_store(
    path: &PathBuf,
    store: &mut Store,
    id: &str,
    name: &str,
) -> Result<String, String> {
    let token = random_hex(32);
    let mut next = store.clone();
    next.devices.retain(|d| d.id != id);
    next.devices.push(Device {
        id: id.into(),
        name: name.into(),
        token: token.clone(),
    });
    save(path, &next)?;
    *store = next;
    Ok(token)
}

pub fn complete(id: &str, name: &str) -> Result<String, String> {
    let mut guard = STORE.lock().unwrap();
    let (path, store) = guard.get_or_insert_with(|| (PathBuf::new(), Store::default()));
    complete_store(path, store, id, name)
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

        let token = complete("tab-1", "Redmi Pad 2").unwrap();
        assert!(is_paired("tab-1", &token));
        assert!(
            !is_paired("tab-1", "nope") && !is_paired("tab-2", &token) && !is_paired("tab-1", "")
        );
        forget("tab-1").unwrap();
        assert!(!is_paired("tab-1", &token));

        start("x");
        for _ in 0..MAX_ATTEMPTS - 1 {
            assert_eq!(check("bad"), Check::Wrong);
        }
        assert_eq!(check("bad"), Check::Over); // too many attempts ends it
    }

    #[test]
    fn pairing_failure_is_reported_before_memory_authorization() {
        let root =
            std::env::temp_dir().join(format!("tabdisplay-pairing-blocker-{}", std::process::id()));
        let blocker = root.join("not-a-directory");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&blocker, b"blocker").unwrap();
        let path = blocker.join("paired.json");
        let mut store = Store::default();

        assert!(complete_store(&path, &mut store, "tab-failure", "tablet").is_err());
        assert!(store.devices.is_empty());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn revocation_failure_does_not_drop_in_memory_authorization() {
        let root =
            std::env::temp_dir().join(format!("tabdisplay-revoke-blocker-{}", std::process::id()));
        let blocker = root.join("not-a-directory");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&blocker, b"blocker").unwrap();
        let path = blocker.join("paired.json");
        let mut store = Store {
            pc_id: String::new(),
            devices: vec![Device {
                id: "tab-revoke".into(),
                name: "tablet".into(),
                token: "secret".into(),
            }],
        };

        assert!(forget_store(&path, &mut store, "tab-revoke").is_err());
        assert!(is_paired_in(&store, "tab-revoke", "secret"));

        std::fs::remove_dir_all(root).unwrap();
    }

    fn is_paired_in(store: &Store, id: &str, token: &str) -> bool {
        store
            .devices
            .iter()
            .any(|device| device.id == id && device.token == token)
    }
}
