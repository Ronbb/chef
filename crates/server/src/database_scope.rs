//! Trusted deployment schema selection, never derived from a request or product host.
use sea_orm::ConnectOptions;

pub fn apply(options: &mut ConnectOptions, schema: Option<&str>) -> anyhow::Result<()> {
    if let Some(schema) = schema {
        validate(schema)?;
        options.set_schema_search_path(schema);
    }
    Ok(())
}

pub fn validate(schema: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !schema.is_empty()
            && schema.len() <= 63
            && schema
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
            && schema.as_bytes()[0].is_ascii_lowercase(),
        "Invalid configured database schema"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schema_is_one_trusted_identifier() {
        for schema in ["chef_identity", "chef_learning", "test_20261008"] {
            assert!(
                apply(
                    &mut ConnectOptions::new("postgres://localhost/test"),
                    Some(schema)
                )
                .is_ok()
            );
        }
        assert!(apply(&mut ConnectOptions::new("postgres://localhost/test"), None).is_ok());
        for schema in [
            "",
            "1schema",
            "Identity",
            "a,b",
            "public;DROP TABLE users",
            "a.b",
            "a\"",
            "身份",
        ] {
            assert!(
                apply(
                    &mut ConnectOptions::new("postgres://localhost/test"),
                    Some(schema)
                )
                .is_err()
            );
        }
        assert!(
            apply(
                &mut ConnectOptions::new("postgres://localhost/test"),
                Some(&"a".repeat(64))
            )
            .is_err()
        );
    }
}
