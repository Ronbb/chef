//! Trusted request authorization shared by legacy and independent content routes.
use crate::{
    AppError, identity::AuthSession, learning_identity::AccountOwner, product_memberships::Operator,
};
use axum::{extract::FromRequestParts, http::request::Parts};

pub(crate) enum AdminAuth {
    Local(AuthSession),
    Remote(crate::learning_identity::RemoteAuthorization),
}
impl<S: Send + Sync> FromRequestParts<S> for AdminAuth {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        if let Some(proof) = parts
            .extensions
            .get::<crate::learning_identity::RemoteAuthorization>()
        {
            return Ok(Self::Remote(proof.clone()));
        }
        AuthSession::from_request_parts(parts, state)
            .await
            .map(Self::Local)
            .map_err(|_| AppError::Unavailable)
    }
}
impl AccountOwner for AdminAuth {
    fn account_id(&self) -> Result<i64, AppError> {
        match self {
            Self::Local(auth) => auth.account_id(),
            Self::Remote(proof) => Ok(proof.actor()),
        }
    }
}
impl AdminAuth {
    pub(crate) async fn require_operator(&self) -> Result<Operator, AppError> {
        match self {
            Self::Local(auth) => crate::identity::require_operator(auth).await,
            Self::Remote(proof) => proof.operator(),
        }
    }
}
