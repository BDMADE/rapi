use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
pub const DOCUMENT_ID_LEN: usize = 24;

/// Strapi v5-compatible 24-character lowercase alphanumeric document identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(transparent)]
pub struct DocumentId(String);

impl DocumentId {
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let mut out = String::with_capacity(DOCUMENT_ID_LEN);
        // Ensure first character is an ASCII lowercase letter (CUID2 convention)
        let first_idx = rng.gen_range(0..26);
        out.push(ALPHABET[first_idx] as char);
        for _ in 1..DOCUMENT_ID_LEN {
            let idx = rng.gen_range(0..ALPHABET.len());
            out.push(ALPHABET[idx] as char);
        }
        Self(out)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for DocumentId {
    fn default() -> Self {
        Self::generate()
    }
}

impl fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for DocumentId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() || s.len() > 64 {
            return Err("documentId must be between 1 and 64 characters");
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err("documentId contains invalid characters");
        }
        Ok(Self(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_24_char_document_id() {
        let id = DocumentId::generate();
        assert_eq!(id.as_str().len(), 24);
        assert!(id.as_str().chars().next().unwrap().is_ascii_lowercase());
        assert!(id
            .as_str()
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
        assert_eq!(DocumentId::from_str(id.as_str()).unwrap(), id);
    }
}
