//! Versioned, bounded wire contract for the future authenticated file channel.
//!
//! This module deliberately does not advertise a capability or open another
//! socket. The server must associate every decoded message with the already
//! authenticated session before handing it to `files::ReceiveDestination`.

use crate::files::{TransferId, TransferMeta, MAX_CHUNK_BYTES, MAX_FILE_BYTES};
use serde::{Deserialize, Serialize};

pub const VERSION: u8 = 1;
pub const MAX_CONTROL_BYTES: usize = 64 << 10;
pub const CHUNK_HEADER_BYTES: usize = 16 + 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Chunk {
    pub id: TransferId,
    pub offset: u64,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct End {
    pub id: TransferId,
    pub sha256: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cancel {
    pub id: TransferId,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    InvalidJson,
    WrongVersion,
    InvalidId,
    InvalidHash,
    InvalidName,
    FileTooLarge,
    ControlTooLarge,
    ChunkTooLarge,
    EmptyChunk,
    TruncatedChunk,
    TrailingChunkBytes,
    InvalidReason,
}

#[derive(Deserialize, Serialize)]
struct BeginWire<'a> {
    version: u8,
    id: String,
    name: &'a str,
    size: u64,
    sha256: String,
}

#[derive(Deserialize, Serialize)]
struct EndWire {
    version: u8,
    id: String,
    sha256: String,
}

#[derive(Deserialize, Serialize)]
struct CancelWire<'a> {
    version: u8,
    id: String,
    reason: &'a str,
}

pub fn encode_begin(meta: &TransferMeta) -> Result<Vec<u8>, ProtocolError> {
    let wire = BeginWire {
        version: VERSION,
        id: hex(meta.id.bytes()),
        name: &meta.name,
        size: meta.size,
        sha256: hex(meta.sha256),
    };
    encode_control(&wire)
}

pub fn decode_begin(payload: &[u8]) -> Result<TransferMeta, ProtocolError> {
    let wire: BeginWire<'_> = decode_control(payload)?;
    check_version(wire.version)?;
    let id = parse_id(&wire.id)?;
    if wire.size > MAX_FILE_BYTES {
        return Err(ProtocolError::FileTooLarge);
    }
    let hash = parse_hash(&wire.sha256)?;
    TransferMeta::new(id.bytes(), wire.name, wire.size, hash).map_err(|error| match error {
        crate::files::MetaError::InvalidId => ProtocolError::InvalidId,
        crate::files::MetaError::InvalidName(_) => ProtocolError::InvalidName,
        crate::files::MetaError::TooLarge => ProtocolError::FileTooLarge,
    })
}

pub fn encode_chunk(chunk: &Chunk) -> Result<Vec<u8>, ProtocolError> {
    if chunk.data.is_empty() {
        return Err(ProtocolError::EmptyChunk);
    }
    if chunk.data.len() > MAX_CHUNK_BYTES {
        return Err(ProtocolError::ChunkTooLarge);
    }
    let mut payload = Vec::with_capacity(CHUNK_HEADER_BYTES + chunk.data.len());
    payload.extend(chunk.id.bytes());
    payload.extend(chunk.offset.to_be_bytes());
    payload.extend(&chunk.data);
    Ok(payload)
}

pub fn decode_chunk(payload: &[u8]) -> Result<Chunk, ProtocolError> {
    if payload.len() < CHUNK_HEADER_BYTES {
        return Err(ProtocolError::TruncatedChunk);
    }
    let data_len = payload.len() - CHUNK_HEADER_BYTES;
    if data_len == 0 {
        return Err(ProtocolError::EmptyChunk);
    }
    if data_len > MAX_CHUNK_BYTES {
        return Err(ProtocolError::ChunkTooLarge);
    }
    let id = parse_id_bytes(&payload[..16])?;
    let mut offset = [0u8; 8];
    offset.copy_from_slice(&payload[16..CHUNK_HEADER_BYTES]);
    Ok(Chunk {
        id,
        offset: u64::from_be_bytes(offset),
        data: payload[CHUNK_HEADER_BYTES..].to_vec(),
    })
}

pub fn encode_end(end: &End) -> Result<Vec<u8>, ProtocolError> {
    encode_control(&EndWire {
        version: VERSION,
        id: hex(end.id.bytes()),
        sha256: hex(end.sha256),
    })
}

pub fn decode_end(payload: &[u8]) -> Result<End, ProtocolError> {
    let wire: EndWire = decode_control(payload)?;
    check_version(wire.version)?;
    Ok(End {
        id: parse_id(&wire.id)?,
        sha256: parse_hash(&wire.sha256)?,
    })
}

pub fn encode_cancel(cancel: &Cancel) -> Result<Vec<u8>, ProtocolError> {
    if cancel.reason.is_empty() || cancel.reason.len() > 256 {
        return Err(ProtocolError::InvalidReason);
    }
    encode_control(&CancelWire {
        version: VERSION,
        id: hex(cancel.id.bytes()),
        reason: &cancel.reason,
    })
}

pub fn decode_cancel(payload: &[u8]) -> Result<Cancel, ProtocolError> {
    let wire: CancelWire<'_> = decode_control(payload)?;
    check_version(wire.version)?;
    if wire.reason.is_empty() || wire.reason.len() > 256 {
        return Err(ProtocolError::InvalidReason);
    }
    Ok(Cancel {
        id: parse_id(&wire.id)?,
        reason: wire.reason.to_owned(),
    })
}

fn encode_control<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtocolError> {
    let bytes = serde_json::to_vec(value).map_err(|_| ProtocolError::InvalidJson)?;
    if bytes.len() > MAX_CONTROL_BYTES {
        return Err(ProtocolError::ControlTooLarge);
    }
    Ok(bytes)
}

fn decode_control<'a, T: Deserialize<'a>>(payload: &'a [u8]) -> Result<T, ProtocolError> {
    if payload.len() > MAX_CONTROL_BYTES {
        return Err(ProtocolError::ControlTooLarge);
    }
    serde_json::from_slice(payload).map_err(|_| ProtocolError::InvalidJson)
}

fn check_version(version: u8) -> Result<(), ProtocolError> {
    (version == VERSION)
        .then_some(())
        .ok_or(ProtocolError::WrongVersion)
}

fn parse_id(value: &str) -> Result<TransferId, ProtocolError> {
    if value.len() != 32 {
        return Err(ProtocolError::InvalidId);
    }
    let mut output = [0u8; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (hex_digit(pair[0])? << 4) | hex_digit(pair[1])?;
    }
    TransferId::from_bytes(output).ok_or(ProtocolError::InvalidId)
}

fn parse_id_bytes(bytes: &[u8]) -> Result<TransferId, ProtocolError> {
    let mut id = [0u8; 16];
    id.copy_from_slice(bytes);
    TransferId::from_bytes(id).ok_or(ProtocolError::InvalidId)
}

fn parse_hash(value: &str) -> Result<[u8; 32], ProtocolError> {
    if value.len() != 64 {
        return Err(ProtocolError::InvalidHash);
    }
    let mut output = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (hex_digit(pair[0])? << 4) | hex_digit(pair[1])?;
    }
    Ok(output)
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hex_digit(value: u8) -> Result<u8, ProtocolError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(ProtocolError::InvalidHash),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn meta() -> TransferMeta {
        let bytes = b"bounded file";
        TransferMeta::new(
            [7; 16],
            "arquivo.txt",
            bytes.len() as u64,
            Sha256::digest(bytes).into(),
        )
        .unwrap()
    }

    #[test]
    fn roundtrips_begin_end_and_chunk_with_bounded_payloads() {
        let meta = meta();
        assert_eq!(decode_begin(&encode_begin(&meta).unwrap()).unwrap(), meta);
        let chunk = Chunk {
            id: meta.id,
            offset: 4,
            data: b"chunk".to_vec(),
        };
        assert_eq!(decode_chunk(&encode_chunk(&chunk).unwrap()).unwrap(), chunk);
        let end = End {
            id: meta.id,
            sha256: meta.sha256,
        };
        assert_eq!(decode_end(&encode_end(&end).unwrap()).unwrap(), end);
        let cancel = Cancel {
            id: meta.id,
            reason: "user".into(),
        };
        assert_eq!(
            decode_cancel(&encode_cancel(&cancel).unwrap()).unwrap(),
            cancel
        );
    }

    #[test]
    fn rejects_wrong_version_ids_hashes_names_and_chunk_limits() {
        let mut begin = encode_begin(&meta()).unwrap();
        let version_digit = begin.iter().position(|byte| *byte == b'1').unwrap();
        begin[version_digit] = b'2';
        assert_eq!(decode_begin(&begin), Err(ProtocolError::WrongVersion));
        let too_large = Chunk {
            id: meta().id,
            offset: 0,
            data: vec![0; MAX_CHUNK_BYTES + 1],
        };
        assert_eq!(encode_chunk(&too_large), Err(ProtocolError::ChunkTooLarge));
        assert_eq!(
            decode_chunk(&[0; CHUNK_HEADER_BYTES]),
            Err(ProtocolError::EmptyChunk)
        );
        assert_eq!(
            decode_begin(br#"{"version":1,"id":"00","name":"x","size":0,"sha256":"00"}"#),
            Err(ProtocolError::InvalidId)
        );
        let invalid_name = br#"{"version":1,"id":"07070707070707070707070707070707","name":"../x","size":0,"sha256":"0000000000000000000000000000000000000000000000000000000000000000"}"#;
        assert_eq!(decode_begin(invalid_name), Err(ProtocolError::InvalidName));
    }
}
