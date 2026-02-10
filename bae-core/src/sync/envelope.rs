/// Changeset envelope: metadata + binary changeset packed into a single blob.
///
/// Wire format: `JSON bytes + \0 + changeset bytes`
///
/// The envelope carries enough context to understand the changeset without
/// unpacking the binary portion (schema version, author, description).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangesetEnvelope {
    pub device_id: String,
    pub seq: u64,
    pub schema_version: u32,
    pub message: String,
    pub timestamp: String,
    pub changeset_size: usize,
}

/// Pack an envelope and changeset into the wire format.
///
/// Layout: `[envelope JSON] \0 [changeset bytes]`
pub fn pack(envelope: &ChangesetEnvelope, changeset: &[u8]) -> Vec<u8> {
    let json = serde_json::to_vec(envelope).expect("envelope serialization cannot fail");
    let mut buf = Vec::with_capacity(json.len() + 1 + changeset.len());
    buf.extend_from_slice(&json);
    buf.push(0);
    buf.extend_from_slice(changeset);
    buf
}

/// Unpack the wire format into envelope + changeset bytes.
///
/// Returns `None` if the format is invalid (no null separator or bad JSON).
pub fn unpack(data: &[u8]) -> Option<(ChangesetEnvelope, Vec<u8>)> {
    let separator = data.iter().position(|&b| b == 0)?;
    let json_bytes = &data[..separator];
    let changeset_bytes = &data[separator + 1..];

    let envelope: ChangesetEnvelope = serde_json::from_slice(json_bytes).ok()?;
    Some((envelope, changeset_bytes.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_envelope() -> ChangesetEnvelope {
        ChangesetEnvelope {
            device_id: "dev-abc123".into(),
            seq: 42,
            schema_version: 2,
            message: "Imported Kind of Blue".into(),
            timestamp: "2026-02-10T14:30:00Z".into(),
            changeset_size: 4096,
        }
    }

    #[test]
    fn pack_unpack_roundtrip() {
        let envelope = test_envelope();
        let changeset = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01, 0x02];

        let packed = pack(&envelope, &changeset);
        let (unpacked_env, unpacked_cs) = unpack(&packed).expect("unpack should succeed");

        assert_eq!(unpacked_env, envelope);
        assert_eq!(unpacked_cs, changeset);
    }

    #[test]
    fn pack_unpack_empty_changeset() {
        let envelope = test_envelope();
        let changeset: Vec<u8> = vec![];

        let packed = pack(&envelope, &changeset);
        let (unpacked_env, unpacked_cs) = unpack(&packed).expect("unpack should succeed");

        assert_eq!(unpacked_env, envelope);
        assert!(unpacked_cs.is_empty());
    }

    #[test]
    fn pack_contains_null_separator() {
        let envelope = test_envelope();
        let changeset = vec![0xFF];

        let packed = pack(&envelope, &changeset);

        // Find the null byte -- it should exist exactly once between JSON and changeset.
        let null_positions: Vec<usize> = packed
            .iter()
            .enumerate()
            .filter(|&(_, &b)| b == 0)
            .map(|(i, _)| i)
            .collect();

        // The changeset doesn't contain 0x00 in this case, so exactly one null.
        assert_eq!(null_positions.len(), 1);
    }

    #[test]
    fn changeset_with_embedded_nulls() {
        let envelope = test_envelope();
        // Changeset bytes that contain null bytes -- unpack should handle this
        // because we split on the FIRST null (after JSON).
        let changeset = vec![0x00, 0x00, 0xFF, 0x00];

        let packed = pack(&envelope, &changeset);
        let (unpacked_env, unpacked_cs) = unpack(&packed).expect("unpack should succeed");

        assert_eq!(unpacked_env, envelope);
        assert_eq!(unpacked_cs, changeset);
    }

    #[test]
    fn unpack_invalid_no_separator() {
        let data = b"hello world";
        assert!(unpack(data).is_none());
    }

    #[test]
    fn unpack_invalid_bad_json() {
        // Null separator present but JSON is invalid
        let mut data = b"not json".to_vec();
        data.push(0);
        data.extend_from_slice(b"changeset");

        assert!(unpack(&data).is_none());
    }

    #[test]
    fn unpack_empty_input() {
        assert!(unpack(&[]).is_none());
    }
}
