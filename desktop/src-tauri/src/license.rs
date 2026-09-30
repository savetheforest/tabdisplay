//! Offline licence: `TDL1.<payload>.<signature>` (see scripts/license.mjs), an Ed25519 signature over
//! `TDL1.<payload>` made with a private key that never leaves the seller. Without a valid licence the app
//! mirrors only; extending the desktop needs one.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Public half of the seller's key (`node scripts/license.mjs keygen`).
const PUBLIC_KEY: [u8; 32] =
    hex(b"0c20a8b32477bae13210cc3b849dba24a60cad7053d7c20b12b44b2c4f5df3d8");
/// Where the "buy" button in the app goes. ponytail: placeholder until the store page exists.
pub const BUY_URL: &str = "https://github.com/savetheforest/tabdisplay";

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Licence {
    pub name: String,
    pub email: String,
    pub issued: String,
}

static CURRENT: Mutex<Option<Licence>> = Mutex::new(None);
static PATH: OnceLock<PathBuf> = OnceLock::new();

const fn digit(c: u8) -> u8 {
    if c <= b'9' {
        c - b'0'
    } else {
        c - b'a' + 10
    }
}

const fn hex(s: &[u8; 64]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = digit(s[2 * i]) << 4 | digit(s[2 * i + 1]);
        i += 1;
    }
    out
}

/// The licence in `token` if its signature checks out against `key`.
fn verify(token: &str, key: &VerifyingKey) -> Result<Licence, &'static str> {
    let token = token.trim();
    let (signed, signature) = token.rsplit_once('.').ok_or("Licença inválida.")?;
    let payload = signed.strip_prefix("TDL1.").ok_or("Licença inválida.")?;
    let signature = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| "Licença inválida.")?;
    let signature = Signature::from_slice(&signature).map_err(|_| "Licença inválida.")?;
    key.verify_strict(signed.as_bytes(), &signature)
        .map_err(|_| "Licença inválida: a assinatura não confere.")?;
    let json = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| "Licença inválida.")?;
    serde_json::from_slice(&json).map_err(|_| "Licença inválida.")
}

fn public_key() -> VerifyingKey {
    VerifyingKey::from_bytes(&PUBLIC_KEY).expect("embedded public key")
}

/// Loads `dir/license.txt` if it holds a valid licence.
pub fn init(dir: PathBuf) {
    let path = dir.join("license.txt");
    let current = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| verify(&t, &public_key()).ok());
    *CURRENT.lock().unwrap() = current;
    let _ = PATH.set(path);
}

pub fn current() -> Option<Licence> {
    CURRENT.lock().unwrap().clone()
}

pub fn valid() -> bool {
    current().is_some()
}

/// Checks and stores a licence; the error text is for the user.
pub fn activate(token: &str) -> Result<Licence, &'static str> {
    let licence = verify(token, &public_key())?;
    if let Some(path) = PATH.get() {
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        std::fs::write(path, token.trim()).map_err(|_| "Não consegui gravar a licença.")?;
    }
    *CURRENT.lock().unwrap() = Some(licence.clone());
    Ok(licence)
}

pub fn remove() {
    if let Some(path) = PATH.get() {
        let _ = std::fs::remove_file(path);
    }
    *CURRENT.lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Issued by `node scripts/license.mjs issue "Teste da Silva" "teste@example.com"` with the real key.
    const TOKEN: &str = "TDL1.eyJuYW1lIjoiVGVzdGUgZGEgU2lsdmEiLCJlbWFpbCI6InRlc3RlQGV4YW1wbGUuY29tIiwiaXNzdWVkIjoiMjAyNi0wOS0yNiJ9.edk6ZNYWHdW_0lZPEyJDiTS_IHbZEteUr0lVTRvNOTmsNUu1IhuZKPLlYnJbXHFr-Nckm_sOxodSDVQlp_bFBw";

    #[test]
    fn accepts_a_licence_from_the_script() {
        let l = verify(TOKEN, &public_key()).unwrap();
        assert_eq!(
            (l.name.as_str(), l.email.as_str(), l.issued.as_str()),
            ("Teste da Silva", "teste@example.com", "2026-09-26")
        );
    }

    #[test]
    fn rejects_tampered_and_foreign_licences() {
        // Different name in the payload, same signature.
        let forged_payload = URL_SAFE_NO_PAD.encode(
            br#"{"name":"Outra Pessoa","email":"teste@example.com","issued":"2026-09-26"}"#,
        );
        let sig = TOKEN.rsplit_once('.').unwrap().1;
        assert!(verify(&format!("TDL1.{forged_payload}.{sig}"), &public_key()).is_err());
        // Flipped signature bit, truncated, garbage.
        let mut flipped = TOKEN.as_bytes().to_vec();
        let last = flipped.len() - 1;
        flipped[last] = if flipped[last] == b'A' { b'B' } else { b'A' };
        assert!(verify(std::str::from_utf8(&flipped).unwrap(), &public_key()).is_err());
        assert!(verify(&TOKEN[..TOKEN.len() - 4], &public_key()).is_err());
        assert!(verify("lixo", &public_key()).is_err());
        // Signed by some other key.
        let other = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        assert!(verify(TOKEN, &other.verifying_key()).is_err());
    }
}
