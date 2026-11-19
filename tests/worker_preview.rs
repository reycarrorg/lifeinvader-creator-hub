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

// Synthetic evidence validation only. None of these types is wired to runtime code.
#[cfg(test)]
mod evidence_tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Selection {
        generation: u64,
        version: Version,
        cut_key: Sha256,
        cut: Cut,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum State {
        Unknown,
        Pending,
        Failed,
        Revoked,
        Ambiguous,
        Committed,
    }

    // Receiver-side request association; the generation must not be supplied by JSON.
    struct WorkerReply {
        request_generation: u64,
        response: UntrustedResponse,
    }

    #[derive(Debug, Clone)]
    struct Ledger {
        selection: Selection,
        state: State,
        receipt: Option<Receipt>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Observation {
        Unsafe,
        Unavailable,
        Unverified,
        Verified { selection: Selection, observed: Receipt },
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Outcome {
        Hold(HoldReason),
        EvidenceMatches,
    }

    fn compare_selection(
        expected: &Selection,
        found: &Selection,
    ) -> Result<(), HoldReason> {
        if found.generation != expected.generation {
            return Err(HoldReason::Stale);
        }
        if found != expected {
            return Err(HoldReason::EvidenceMismatch);
        }
        Ok(())
    }

    fn check(
        selection: Option<&Selection>,
        response: Option<&WorkerReply>,
        ledger: Option<&Ledger>,
        artifact: Option<&Observation>,
    ) -> Result<(), HoldReason> {
        let selection = selection.ok_or(HoldReason::MissingEvidence)?;
        if selection.generation == 0 || !selection.cut.valid() {
            return Err(HoldReason::VerificationUnavailable);
        }
        let ledger = ledger.ok_or(HoldReason::MissingEvidence)?;
        compare_selection(selection, &ledger.selection)?;
        match ledger.state {
            State::Unknown => return Err(HoldReason::VerificationUnavailable),
            State::Pending => return Err(HoldReason::Pending),
            State::Failed => return Err(HoldReason::Failed),
            State::Revoked => return Err(HoldReason::Revoked),
            State::Ambiguous => return Err(HoldReason::Ambiguous),
            State::Committed => {}
        }
        let receipt = ledger.receipt.as_ref().ok_or(HoldReason::MissingEvidence)?;
        if receipt.byte_count == 0 {
            return Err(HoldReason::VerificationUnavailable);
        }
        let response = response.ok_or(HoldReason::MissingEvidence)?;
        if response.request_generation != selection.generation {
            return Err(HoldReason::Stale);
        }
        let ready = match &response.response.0 {
            WireResponse::Hold(hold) => return Err(hold.reason.clone()),
            WireResponse::PreviewReady(ready) => ready,
        };
        if ready.schema_version != selection.version
            || ready.cut_key != selection.cut_key
            || ready.cut != selection.cut
            || ready.receipt != *receipt
        {
            return Err(HoldReason::EvidenceMismatch);
        }
        match artifact.ok_or(HoldReason::MissingEvidence)? {
            Observation::Unsafe => Err(HoldReason::UnsafeArtifact),
            Observation::Unavailable => Err(HoldReason::ArtifactUnavailable),
            Observation::Unverified => Err(HoldReason::VerificationUnavailable),
            Observation::Verified { selection: observed_selection, observed } => {
                compare_selection(selection, observed_selection)?;
                if observed != receipt {
                    return Err(HoldReason::ArtifactMismatch);
                }
                Ok(())
            }
        }
    }

    fn evaluate(
        selection: Option<&Selection>,
        response: Option<&WorkerReply>,
        ledger: Option<&Ledger>,
        artifact: Option<&Observation>,
    ) -> Outcome {
        match check(selection, response, ledger, artifact) {
            Ok(()) => Outcome::EvidenceMatches,
            Err(reason) => Outcome::Hold(reason),
        }
    }

    struct Fixture {
        selection: Option<Selection>,
        claim: Option<WorkerReply>,
        ledger: Option<Ledger>,
        artifact: Option<Observation>,
    }

    fn hash(byte: char) -> Sha256 {
        Sha256::try_from(byte.to_string().repeat(64)).unwrap()
    }

    impl Fixture {
        fn new() -> Self {
            // Each evidence channel has its own fixture, not a value copied from a wire claim.
            fn selection() -> Selection {
                Selection {
                    generation: 7,
                    version: Version,
                    cut_key: hash('a'),
                    cut: Cut {
                        provider: "fixture".into(),
                        provider_id: "synthetic-item".into(),
                        source_sha256: hash('b'),
                        start_ms: 1000,
                        end_ms: 22000,
                        recipe_version: "fixture-v1".into(),
                    },
                }
            }
            fn receipt() -> Receipt {
                Receipt { artifact_sha256: hash('c'), byte_count: 4 }
            }
            let encoded = format!(
                r#"{{"preview_ready":{{"schema_version":1,"cut_key":"{a}","cut":{{"provider":"fixture","provider_id":"synthetic-item","source_sha256":"{b}","start_ms":1000,"end_ms":22000,"recipe_version":"fixture-v1"}},"receipt":{{"artifact_sha256":"{c}","byte_count":4}}}}}}"#,
                a = "a".repeat(64), b = "b".repeat(64), c = "c".repeat(64),
            );
            Self {
                selection: Some(selection()),
                claim: Some(WorkerReply {
                    request_generation: 7,
                    response: parse_response(encoded.as_bytes()).unwrap(),
                }),
                ledger: Some(Ledger {
                    selection: selection(),
                    state: State::Committed,
                    receipt: Some(receipt()),
                }),
                artifact: Some(Observation::Verified {
                    selection: selection(),
                    observed: receipt(),
                }),
            }
        }
        fn check(&self) -> Outcome {
            evaluate(
                self.selection.as_ref(), self.claim.as_ref(),
                self.ledger.as_ref(), self.artifact.as_ref(),
            )
        }
        fn ready_claim(&mut self) -> &mut ReadyClaim {
            match &mut self.claim.as_mut().unwrap().response.0 {
                WireResponse::PreviewReady(ready) => ready,
                _ => panic!("expected ready fixture"),
            }
        }
        fn observation(&mut self) -> (&mut Selection, &mut Receipt) {
            match self.artifact.as_mut().unwrap() {
                Observation::Verified { selection, observed } => (selection, observed),
                _ => panic!("expected verified fixture"),
            }
        }
    }

    #[test]
    fn exact_match_is_repeatable() {
        let f = Fixture::new();
        assert_eq!(f.check(), Outcome::EvidenceMatches);
        assert_eq!(f.check(), Outcome::EvidenceMatches);
    }

    #[test]
    fn no_channel_can_be_omitted() {
        for drop in [
            (|f: &mut Fixture| f.selection = None) as fn(&mut Fixture),
            |f| f.claim = None,
            |f| f.ledger = None,
            |f| f.artifact = None,
            |f| f.ledger.as_mut().unwrap().receipt = None,
        ] {
            let mut f = Fixture::new();
            drop(&mut f);
            assert_eq!(f.check(), Outcome::Hold(HoldReason::MissingEvidence));
        }
    }

    #[test]
    fn ready_claim_cannot_override_ledger_state() {
        for (state, reason) in [
            (State::Unknown, HoldReason::VerificationUnavailable),
            (State::Pending, HoldReason::Pending),
            (State::Failed, HoldReason::Failed),
            (State::Revoked, HoldReason::Revoked),
            (State::Ambiguous, HoldReason::Ambiguous),
        ] {
            let mut f = Fixture::new();
            f.ledger.as_mut().unwrap().state = state;
            assert_eq!(f.check(), Outcome::Hold(reason));
        }
    }

    #[test]
    fn old_and_future_generations_hold_even_when_same_cut() {
        for generation in [6, 8] {
            let mut f = Fixture::new();
            f.claim.as_mut().unwrap().request_generation = generation;
            assert_eq!(f.check(), Outcome::Hold(HoldReason::Stale));
            let mut f = Fixture::new();
            f.ledger.as_mut().unwrap().selection.generation = generation;
            assert_eq!(f.check(), Outcome::Hold(HoldReason::Stale));
            let mut f = Fixture::new();
            f.observation().0.generation = generation;
            assert_eq!(f.check(), Outcome::Hold(HoldReason::Stale));
        }
    }

    #[test]
    fn each_canonical_cut_field_and_key_must_match() {
        for mutate in [
            (|c: &mut Cut| c.provider = "other".into()) as fn(&mut Cut),
            |c| c.provider_id = "other".into(),
            |c| c.source_sha256 = hash('d'),
            |c| c.start_ms += 1,
            |c| c.end_ms += 1,
            |c| c.recipe_version = "fixture-v2".into(),
        ] {
            let mut f = Fixture::new();
            mutate(&mut f.ready_claim().cut);
            assert_eq!(f.check(), Outcome::Hold(HoldReason::EvidenceMismatch));
            let mut f = Fixture::new();
            mutate(&mut f.ledger.as_mut().unwrap().selection.cut);
            assert_eq!(f.check(), Outcome::Hold(HoldReason::EvidenceMismatch));
            let mut f = Fixture::new();
            mutate(&mut f.observation().0.cut);
            assert_eq!(f.check(), Outcome::Hold(HoldReason::EvidenceMismatch));
        }
        let mut f = Fixture::new();
        f.ready_claim().cut_key = hash('d');
        assert_eq!(f.check(), Outcome::Hold(HoldReason::EvidenceMismatch));
        let mut f = Fixture::new();
        f.ledger.as_mut().unwrap().selection.cut_key = hash('d');
        assert_eq!(f.check(), Outcome::Hold(HoldReason::EvidenceMismatch));
        let mut f = Fixture::new();
        f.observation().0.cut_key = hash('d');
        assert_eq!(f.check(), Outcome::Hold(HoldReason::EvidenceMismatch));
    }

    #[test]
    fn receipt_bytes_and_hash_must_agree_independently() {
        for mutate in [
            (|r: &mut Receipt| r.byte_count += 1) as fn(&mut Receipt),
            |r| r.artifact_sha256 = hash('e'),
        ] {
            let mut f = Fixture::new();
            mutate(&mut f.ready_claim().receipt);
            assert_eq!(f.check(), Outcome::Hold(HoldReason::EvidenceMismatch));
            let mut f = Fixture::new();
            mutate(f.observation().1);
            assert_eq!(f.check(), Outcome::Hold(HoldReason::ArtifactMismatch));
        }
        let mut f = Fixture::new();
        f.ledger.as_mut().unwrap().receipt.as_mut().unwrap().byte_count = 0;
        assert_eq!(f.check(), Outcome::Hold(HoldReason::VerificationUnavailable));
    }

    #[test]
    fn unverified_unavailable_and_unsafe_observations_are_closed_holds() {
        for (observation, reason) in [
            (Observation::Unverified, HoldReason::VerificationUnavailable),
            (Observation::Unavailable, HoldReason::ArtifactUnavailable),
            (Observation::Unsafe, HoldReason::UnsafeArtifact),
        ] {
            let mut f = Fixture::new();
            f.artifact = Some(observation);
            assert_eq!(f.check(), Outcome::Hold(reason));
        }
    }

    #[test]
    fn worker_hold_is_not_promoted_and_closed_diagnostics_do_not_echo_cut() {
        let mut f = Fixture::new();
        f.claim = Some(WorkerReply {
            request_generation: 7,
            response: parse_response(
                br#"{"hold":{"schema_version":1,"reason":"pending"}}"#
            ).unwrap(),
        });
        assert_eq!(f.check(), Outcome::Hold(HoldReason::Pending));
        assert_eq!(format!("{:?}", f.check()), "Hold(Pending)");
    }

    #[test]
    fn invalid_current_generation_holds() {
        let mut f = Fixture::new();
        f.selection.as_mut().unwrap().generation = 0;
        assert_eq!(f.check(), Outcome::Hold(HoldReason::VerificationUnavailable));
    }
}
