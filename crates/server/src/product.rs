use serde::{Deserialize, Serialize};

/// Trusted deployment context, never selected by a browser request parameter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProductId {
    Brioche,
    Hargow,
}
impl ProductId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Brioche => "brioche",
            Self::Hargow => "hargow",
        }
    }
    pub fn cookie_name(self, secure: bool) -> &'static str {
        match (self, secure) {
            (Self::Brioche, true) => "__Host-brioche.sid",
            (Self::Brioche, false) => "brioche.sid",
            (Self::Hargow, true) => "__Host-hargow.sid",
            (Self::Hargow, false) => "hargow.sid",
        }
    }
}
