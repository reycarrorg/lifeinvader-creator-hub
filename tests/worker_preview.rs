//! Draft wire contract only. Parsing never authorizes a preview.
//! No filesystem, ledger, process, network, UI, or queue operations.

use serde::{Deserialize, Serialize};

const MAX_RECORD_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
struct Version;

impl TryFrom<u32> for Version {
    type Error = &'static str;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value == 1 {
            Ok(Self)
        } else {
            Err("unsupported version")
        }
    }
}

impl From<Version> for u32 {
    fn from(_: Version) -> Self {
        1
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
struct Sha256(String);

impl TryFrom<String> for Sha256 {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() == 64
            && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            Ok(Self(value))
        } else {
            Err("invalid hash")
        }
    }
}

impl From<Sha256> for String {
    fn from(value: Sha256) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cut {
    provider: String,
    provider_id: String,
    source_sha256: Sha256,
    start_ms: u64,
    end_ms: u64,
    recipe_version: String,
}

impl Cut {
    fn valid(&self) -> bool {
        fn identifier(value: &str) -> bool {
            !value.is_empty()
                && value.len() <= 256
                && value.bytes().all(|b| {
                    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')
                })
        }
        identifier(&self.provider)
            && identifier(&self.provider_id)
            && identifier(&self.recipe_version)
            && self.start_ms < self.end_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    artifact_sha256: Sha256,
    byte_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum HoldReason {
    MissingEvidence,
    Pending,
    Failed,
    Stale,
    Revoked,
    Ambiguous,
    EvidenceMismatch,
    ArtifactUnavailable,
    ArtifactMismatch,
    UnsafeArtifact,
    VerificationUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Hold {
    schema_version: Version,
    reason: HoldReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadyClaim {
    schema_version: Version,
    cut_key: Sha256,
    cut: Cut,
    receipt: Receipt,
}

/// External tagging keeps hold and ready payloads mutually exclusive.
/// The cut-key encoding/derivation must be agreed with the worker before
/// integration; syntactic hash validation does not prove tuple binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireResponse {
    Hold(Hold),
    PreviewReady(ReadyClaim),
}

/// Opaque, syntactically validated, UNTRUSTED response.
/// Deliberately offers no readiness or authorization accessor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UntrustedResponse(WireResponse);

pub fn parse_response(bytes: &[u8]) -> Result<UntrustedResponse, InvalidRecord> {
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(InvalidRecord);
    }
    // Parse directly into structs: never collapse duplicate keys via Value.
    let response: WireResponse =
        serde_json::from_slice(bytes).map_err(|_| InvalidRecord)?;
    if let WireResponse::PreviewReady(ready) = &response {
        if !ready.cut.valid() || ready.receipt.byte_count == 0 {
            return Err(InvalidRecord);
        }
    }
    Ok(UntrustedResponse(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> String {
        format!(
            r#"{{"preview_ready":{{
                "schema_version":1,
                "cut_key":"{a}",
                "cut":{{
                    "provider":"fixture",
                    "provider_id":"synthetic-item",
                    "source_sha256":"{b}",
                    "start_ms":0,
                    "end_ms":1000,
                    "recipe_version":"fixture-v1"
                }},
                "receipt":{{"artifact_sha256":"{c}","byte_count":4}}
            }}}}"#,
            a = "a".repeat(64),
            b = "b".repeat(64),
            c = "c".repeat(64),
        )
    }

    fn rejects(value: &str) {
        assert_eq!(parse_response(value.as_bytes()), Err(InvalidRecord));
    }

    #[test]
    fn accepts_synthetic_wire_shapes_without_authorizing_them() {
        assert!(parse_response(ready().as_bytes()).is_ok());
        assert!(parse_response(
            br#"{"hold":{"schema_version":1,"reason":"verification_unavailable"}}"#
        ).is_ok());
    }

    #[test]
    fn rejects_duplicate_and_unknown_fields() {
        let value = ready();
        rejects(&value.replace(
            "\"schema_version\":1",
            "\"schema_version\":1,\"schema_version\":1",
        ));
        rejects(&value.replace("\"start_ms\":0", "\"start_ms\":0,\"start_ms\":0"));
        rejects(&value.replace("\"byte_count\":4", "\"byte_count\":4,\"byte_count\":4"));
        rejects(&value.replace("\"start_ms\":0", "\"unknown\":true,\"start_ms\":0"));
        rejects(r#"{"hold":{"schema_version":1,"reason":"pending","path":"secret"}}"#);
        rejects(r#"{"hold":{"schema_version":1,"reason":null,"reason":"pending"}}"#);
    }

    #[test]
    fn rejects_versions_types_ranges_and_hashes() {
        let value = ready();
        for replacement in ["0", "2", "null", "true", "\"1\"", "1.5"] {
            rejects(&value.replace("\"schema_version\":1",
                &format!("\"schema_version\":{replacement}")));
        }
        for replacement in ["-1", "0", "0.5", "\"1000\"", "18446744073709551616"] {
            rejects(&value.replace("\"end_ms\":1000",
                &format!("\"end_ms\":{replacement}")));
        }
        rejects(&value.replace("\"byte_count\":4", "\"byte_count\":0"));
        rejects(&value.replace(&"a".repeat(64), "invalid"));
        rejects(&value.replace(&"b".repeat(64), &"B".repeat(64)));
        rejects(&value.replace("\"provider_id\":\"synthetic-item\"",
            "\"provider_id\":\"\""));
    }

    #[test]
    fn rejects_inconsistent_or_malformed_records() {
        rejects("{}");
        rejects("[]");
        rejects(r#"{"hold":{"schema_version":1,"reason":"unknown"}}"#);
        rejects(r#"{"hold":{"schema_version":1}}"#);
        rejects(r#"{"hold":{"schema_version":1,"reason":"pending"},"preview_ready":{}}"#);
        rejects(&(ready() + "{}"));
        rejects(&" ".repeat(MAX_RECORD_BYTES + 1));
        assert_eq!(parse_response(&[0xff]), Err(InvalidRecord));
    }

    #[test]
    fn diagnostics_do_not_echo_input() {
        let result = parse_response(br#"{"private":"do-not-echo"}"#);
        assert_eq!(format!("{:?}", result.unwrap_err()), "InvalidRecord");
    }

    #[test]
    fn serialization_is_deterministic_and_round_trips() {
        let parsed = parse_response(ready().as_bytes()).unwrap();
        let first = serde_json::to_vec(&parsed.0).unwrap();
        let second = serde_json::to_vec(&parsed.0).unwrap();
        assert_eq!(first, second);
        assert_eq!(parse_response(&first).unwrap(), parsed);
    }
}
