use serde::{Deserialize, Serialize};

/// Trusted deployment context, never selected by a browser request parameter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProductId {
    Brioche,
    Hargow,
}
impl ProductId {
    /// Parse only trusted process configuration; do not accept aliases or HTTP input.
    pub fn configured(variable: &str) -> anyhow::Result<Self> {
        match std::env::var(variable) {
            Ok(value) => Self::from_configuration(Some(&value)),
            Err(std::env::VarError::NotPresent) => Self::from_configuration(None),
            Err(_) => anyhow::bail!("Invalid configured product"),
        }
    }
    pub fn from_configuration(value: Option<&str>) -> anyhow::Result<Self> {
        match value {
            None | Some("brioche") => Ok(Self::Brioche),
            Some("hargow") => Ok(Self::Hargow),
            _ => anyhow::bail!("Invalid configured product"),
        }
    }
    // Old local commands cannot silently write Brioche when configured for Hargow.
    pub(crate) fn validate_command(self, command: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            self == Self::Brioche
                || matches!(
                    command,
                    "serve"
                        | "check"
                        | "check-release"
                        | "asset-check"
                        | "audio-check"
                        | "assets-check"
                        | "audio-bundle-check"
                        | "speech-plan"
                        | "speech-package-local"
                        | "import"
                        | "release-stage"
                        | "release-activate"
                        | "release-status"
                        | "content-withdraw"
                        | "assets-import"
                        | "audio-import"
                        | "speech-plan-export"
                        | "speech-plan-export-direct"
                        | "speech-package-automatic"
                        | "speech-plan-preview"
                        | "speech-plan-save"
                        | "speech-clip-generate"
                        | "voice-audition-generate"
                        | "voice-audition-review"
                        | "speech-clip-review"
                        | "character-voice-import"
                        | "speech-alignment-import"
                        | "speech-package-import"
                        | "lesson-direct-publication"
                        | "lesson-direct-publication-owner"
                ),
            "Product-scoped command migration incomplete"
        );
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_product_is_exact_and_never_echoes_untrusted_configuration() {
        assert_eq!(
            ProductId::from_configuration(None).unwrap(),
            ProductId::Brioche
        );
        for (value, product) in [
            ("brioche", ProductId::Brioche),
            ("hargow", ProductId::Hargow),
        ] {
            assert_eq!(ProductId::from_configuration(Some(value)).unwrap(), product);
        }
        for value in [
            "",
            "Hargow",
            " hargow",
            "hargow ",
            "arbitrary-secret",
            "brioche,hargow",
        ] {
            assert_eq!(
                ProductId::from_configuration(Some(value))
                    .unwrap_err()
                    .to_string(),
                "Invalid configured product"
            );
        }
        for command in [
            "serve",
            "check",
            "asset-check",
            "speech-plan",
            "import",
            "speech-clip-generate",
            "release-activate",
        ] {
            ProductId::Hargow.validate_command(command).unwrap();
        }
        for command in [
            "migrate",
            "invite",
            "split-identity-schema",
            "migrate-layout",
        ] {
            assert!(ProductId::Hargow.validate_command(command).is_err());
            ProductId::Brioche.validate_command(command).unwrap();
        }
    }
}
