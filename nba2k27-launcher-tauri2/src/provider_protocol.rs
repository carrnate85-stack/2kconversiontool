//! Exact WPF/native-loader binary transport contract. This module has no pipe,
//! process, file or game access. Authentication and owned asset routing must be
//! supplied by the future provider before enabling a game launch.
#![allow(dead_code)] // Unreleased provider port; not connected to the launcher yet.
use std::fmt;
pub(crate) const MAX_FRAME: usize = 2 * 1024 * 1024;
// This prefix is validated literally by the proven C++ loader binary.
pub(crate) const PIPE_PREFIX: &str = r"\\.\pipe\NBA2K27ModProvider-";
pub(crate) const MAX_READ: u32 = 1024 * 1024;
pub(crate) const REQUEST_HEADER: usize = 24;
pub(crate) const RESPONSE_HEADER: usize = 32;
const REQUEST_MAGIC: u32 = 0x31504232;
const RESPONSE_MAGIC: u32 = 0x31524232;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Invalid;
impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Invalid or oversized provider message")
    }
}
impl std::error::Error for Invalid {}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Request {
    pub operation: u16,
    pub offset: u64,
    pub length: u32,
    pub path: String,
}
pub(crate) fn has_magic(bytes: &[u8]) -> bool {
    bytes.get(..4) == Some(REQUEST_MAGIC.to_le_bytes().as_slice())
}
pub(crate) fn parse_request(bytes: &[u8]) -> Result<Request, Invalid> {
    if bytes.len() < REQUEST_HEADER || !has_magic(bytes) {
        return Err(Invalid);
    }
    let version = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
    let operation = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
    let offset = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let length = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let path_len = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
    if version != 1
        || !matches!(operation, 1 | 2)
        || !(1..=260).contains(&path_len)
        || bytes.len() != REQUEST_HEADER + path_len
        || (operation == 1 && (offset != 0 || length != 0))
        || (operation == 2 && length > MAX_READ)
    {
        return Err(Invalid);
    }
    let path = std::str::from_utf8(&bytes[REQUEST_HEADER..])
        .map_err(|_| Invalid)?
        .into();
    Ok(Request {
        operation,
        offset,
        length,
        path,
    })
}
pub(crate) fn encode_success(
    operation: u16,
    size: u64,
    payload: &[u8],
) -> Result<Vec<u8>, Invalid> {
    if payload.len() > MAX_READ as usize {
        return Err(Invalid);
    }
    let mut bytes = vec![0; RESPONSE_HEADER + payload.len()];
    bytes[..4].copy_from_slice(&RESPONSE_MAGIC.to_le_bytes());
    bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
    bytes[6..8].copy_from_slice(&operation.to_le_bytes());
    bytes[16..24].copy_from_slice(&size.to_le_bytes());
    bytes[24..28].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes[RESPONSE_HEADER..].copy_from_slice(payload);
    Ok(bytes)
}
pub(crate) fn encode_error(status: u32) -> Vec<u8> {
    let mut bytes = vec![0; RESPONSE_HEADER];
    bytes[..4].copy_from_slice(&RESPONSE_MAGIC.to_le_bytes());
    bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
    bytes[8..12].copy_from_slice(&status.to_le_bytes());
    bytes
}
pub(crate) fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, Invalid> {
    if payload.is_empty() || payload.len() > MAX_FRAME {
        return Err(Invalid);
    }
    let mut bytes = Vec::with_capacity(4 + payload.len());
    bytes.extend((payload.len() as u32).to_le_bytes());
    bytes.extend(payload);
    Ok(bytes)
}
pub(crate) fn decode_frame(frame: &[u8]) -> Result<&[u8], Invalid> {
    let header = frame.get(..4).ok_or(Invalid)?;
    let size = u32::from_le_bytes(header.try_into().unwrap()) as usize;
    if size == 0 || size > MAX_FRAME || frame.len() != size + 4 {
        return Err(Invalid);
    }
    Ok(&frame[4..])
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    fn oracle() -> Value {
        serde_json::from_str(include_str!("../assets/provider-wire-regressions.json")).unwrap()
    }
    fn bytes(v: &Value) -> Vec<u8> {
        v.as_str()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
            .collect()
    }
    #[test]
    fn native_loader_requests_match_original_provider() {
        let oracle = oracle();
        assert_eq!(oracle["schema"], "nba2k27-provider-wire-oracle-v1");
        let cases = oracle["requests"].as_array().unwrap();
        assert_eq!(cases.len(), 650);
        for (i, c) in cases.iter().enumerate() {
            let actual = parse_request(&bytes(&c["bytes"]));
            assert_eq!(actual.is_ok(), c["ok"] == true, "case {i}");
            if let Ok(r) = actual {
                assert_eq!(
                    r.operation as u64,
                    c["operation"].as_u64().unwrap(),
                    "case {i}"
                );
                assert_eq!(
                    r.offset.to_string(),
                    c["offset"].as_str().unwrap(),
                    "case {i}"
                );
                assert_eq!(r.length as u64, c["length"].as_u64().unwrap(), "case {i}");
                assert_eq!(r.path, c["path"].as_str().unwrap(), "case {i}");
            } else {
                assert_eq!(c["status"], 3, "case {i}");
            }
        }
    }
    #[test]
    fn native_loader_responses_match_original_bytes() {
        let oracle = oracle();
        for c in oracle["responses"].as_array().unwrap() {
            let size = c["size"].as_str().unwrap().parse().unwrap();
            let actual = encode_success(
                c["operation"].as_u64().unwrap() as u16,
                size,
                &bytes(&c["payload"]),
            )
            .unwrap();
            assert_eq!(actual, bytes(&c["bytes"]));
            assert_eq!(actual[12..16], [0; 4]);
            assert_eq!(actual[28..32], [0; 4]);
        }
        for c in oracle["errors"].as_array().unwrap() {
            assert_eq!(
                encode_error(c["status"].as_u64().unwrap() as u32),
                bytes(&c["bytes"])
            );
        }
    }
    #[test]
    fn endpoint_prefix_matches_production_native_loader_contract() {
        let source = include_str!("../../nba2k27-version-proxy/src/version_proxy.cpp");
        assert!(
            source
                .contains(r#"constexpr char kPipePrefix[] = "\\\\.\\pipe\\NBA2K27ModProvider-";"#)
        );
        assert_eq!(PIPE_PREFIX, r"\\.\pipe\NBA2K27ModProvider-");
        assert!(PIPE_PREFIX.len() + 64 <= 192);
    }
    #[test]
    fn framing_and_maximum_read_are_bounded_before_allocation() {
        for n in [1, 257, MAX_READ as usize, MAX_FRAME] {
            let payload = vec![42; n];
            let frame = encode_frame(&payload).unwrap();
            assert_eq!(decode_frame(&frame).unwrap(), payload);
            assert!(decode_frame(&frame[..frame.len() - 1]).is_err());
            let mut extra = frame;
            extra.push(0);
            assert!(decode_frame(&extra).is_err());
        }
        assert!(encode_frame(&[]).is_err());
        assert!(encode_frame(&vec![0; MAX_FRAME + 1]).is_err());
        for header in [0u32, (MAX_FRAME + 1) as u32, u32::MAX] {
            assert!(decode_frame(&header.to_le_bytes()).is_err());
        }
        assert!(decode_frame(&[0, 0, 0]).is_err());
        assert!(encode_success(2, 0, &vec![0; MAX_READ as usize + 1]).is_err());
        assert_eq!(
            encode_success(2, MAX_READ as u64, &vec![0; MAX_READ as usize])
                .unwrap()
                .len(),
            RESPONSE_HEADER + MAX_READ as usize
        );
    }
}
