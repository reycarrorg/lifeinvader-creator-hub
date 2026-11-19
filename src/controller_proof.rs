#![forbid(unsafe_code)]
use serde::{Deserialize,Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct Hash256([u8; 32]);

impl TryFrom<String> for Hash256 {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value).ok_or("noncanonical sha256")
    }
}

impl Serialize for Hash256 {fn serialize<S:serde::Serializer>(&self,s:S)->Result<S::Ok,S::Error>{s.serialize_str(&self.hex())}}

impl Hash256 {
    pub(crate) fn hex(&self)->String {self.0.iter().map(|v|format!("{v:02x}")).collect()}

    pub fn parse(value: &str) -> Option<Self> {
        fn nybble(b: u8) -> Option<u8> {
            match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                _ => None,
            }
        }
        if value.len() != 64 {
            return None;
        }
        let mut bytes = [0; 32];
        for (i, p) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[i] = (nybble(p[0])? << 4) | nybble(p[1])?;
        }
        Some(Self(bytes))
    }
}

fn identifier(value: &str, max: usize, colons: bool) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value.bytes().all(|b| b.is_ascii_alphanumeric()
            || matches!(b, b'_' | b'-' | b'.')
            || (colons && b == b':'))
}

#[derive(Debug, Clone, PartialEq, Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedCut {
    pub generation: u64,
    pub provider: String,
    pub provider_id: String,
    pub source_sha256: Hash256,
    pub source_revision: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub recipe_version: String,
    pub durable_cut_key: Hash256,
}

impl SelectedCut {
    fn valid(&self) -> bool {
        self.generation != 0
            && identifier(&self.provider, 256, false)
            && identifier(&self.provider_id, 256, false)
            && identifier(&self.source_revision, 256, true)
            && self.start_ms < self.end_ms
            && identifier(&self.recipe_version, 256, false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactProof {
    pub sha256: Hash256,
    pub bytes: u64,
}

/// Construct only after independently reading a unique committed durable row.
/// Do not construct this from a controller message or request body.
pub struct CommittedReceipt {
    pub selection: SelectedCut,
    pub artifact: ArtifactProof,
}

