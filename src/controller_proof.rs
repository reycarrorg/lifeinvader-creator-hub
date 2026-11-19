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
    /// Call on authenticated source input before building/signing a Review.
    /// Never normalize an already reviewed registry record or receipt bytes.
    pub fn from_authenticated_profile(mut source: Self) -> Option<Self> {
        source.recipe_version = profile_recipe_base(&source.recipe_version, source.start_ms, source.end_ms)?.to_owned();
        source.valid().then_some(source)
    }

    fn valid(&self) -> bool {
        self.generation != 0
            && identifier(&self.provider, 256, false)
            && identifier(&self.provider_id, 256, false)
            && identifier(&self.source_revision, 256, true)
            && self.start_ms < self.end_ms
            && identifier(&self.recipe_version, 256, false)
    }
}

pub fn profile_recipe_base(raw: &str, start: u64, end: u64) -> Option<&str> {
    if start >= end || raw.len() > 320 { return None; }
    let base = match raw.split_once(":cut=") {
        Some((base, suffix)) if suffix == format!("{start}-{end}") => base,
        Some(_) => return None,
        None => raw,
    };
    identifier(base, 256, false).then_some(base)
}

pub fn canonical_recipe_base(raw: &str) -> bool { identifier(raw, 256, false) }

#[cfg(test)]
mod recipe_tests {
    use super::*;
    #[test] fn recipe_base_and_exact_suffix_agree() {
        assert_eq!(profile_recipe_base("recipe-v1",1290,33720),Some("recipe-v1"));
        assert_eq!(profile_recipe_base("recipe-v1:cut=1290-33720",1290,33720),Some("recipe-v1"));
    }
    #[test] fn recipe_rejects_conflicting_and_duplicate_suffix() {
        for s in ["recipe:cut=0-33720","recipe:cut=1290-33720:cut=1290-33720","recipe:cut=01290-33720","recipe:cut=1290-33720x"] {
            assert_eq!(profile_recipe_base(s,1290,33720),None);
        }
    }
    #[test] fn recipe_rejects_invalid_bounds_and_base() {
        for s in ["",":cut=1-2","a b","a/b","a:b","récipe"] { assert_eq!(profile_recipe_base(s,1,2),None); }
        assert_eq!(profile_recipe_base("recipe",2,2),None);
        assert_eq!(profile_recipe_base("recipe",3,2),None);
        assert_eq!(profile_recipe_base(&"a".repeat(257),1,2),None);
        assert_eq!(profile_recipe_base(&"a".repeat(321),1,2),None);
    }
    #[test] fn recipe_normalizes_before_review_and_receipt_compares_once() {
        let (_,b,_,mut db,receipt,_,_)=crate::controller_worker::tests::fixture();
        let mut s=b.selection;
        s.recipe_version="recipe:cut=0-1000".into();
        assert!(!canonical_recipe_base(&s.recipe_version));
        let s=SelectedCut::from_authenticated_profile(s).unwrap();
        assert_eq!(s.recipe_version,"recipe");
        assert!(crate::controller_receipt::read_native_receipt(&mut db,receipt,&s,&s.source_sha256.hex(),&s.durable_cut_key.hex(),1024).is_ok());
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

