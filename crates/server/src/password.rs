//! Bounded blocking workers keep password hashing off the asynchronous executor.
use crate::AppError;
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct PasswordService {
    permits: Arc<Semaphore>,
}
impl Default for PasswordService {
    fn default() -> Self {
        Self {
            permits: Arc::new(Semaphore::new(2)),
        }
    }
}
impl PasswordService {
    pub async fn hash(&self, password: String) -> Result<String, AppError> {
        if !(12..=128).contains(&password.chars().count())
            || password.len() > 512
            || password.trim().is_empty()
        {
            return Err(AppError::InvalidInput);
        }
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| AppError::RateLimited)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            Argon2::default()
                .hash_password(password.as_bytes())
                .map(|hash| hash.to_string())
                .map_err(|_| AppError::Unavailable)
        })
        .await
        .map_err(|_| AppError::Unavailable)?
    }
    pub async fn verify(&self, password: String, encoded: String) -> Result<bool, AppError> {
        if password.len() > 512 {
            return Ok(false);
        }
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| AppError::RateLimited)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let hash = PasswordHash::new(&encoded).map_err(|_| AppError::Unavailable)?;
            Ok(Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .is_ok())
        })
        .await
        .map_err(|_| AppError::Unavailable)?
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn salts_and_password_boundaries() {
        let service = PasswordService::default();
        let password = "une phrase secrète assez longue".to_owned();
        let first = service.hash(password.clone()).await.unwrap();
        let second = service.hash(password.clone()).await.unwrap();
        assert!(first.starts_with("$argon2id$v=19$"));
        assert_ne!(first, second);
        assert!(service.verify(password, first.clone()).await.unwrap());
        assert!(
            !service
                .verify("wrong password".into(), first)
                .await
                .unwrap()
        );
        assert!(matches!(
            service.hash("short".into()).await,
            Err(AppError::InvalidInput)
        ));
        assert!(matches!(
            service.hash("x".repeat(129)).await,
            Err(AppError::InvalidInput)
        ));
    }
}
