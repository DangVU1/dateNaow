//! Argon2id password hashing (P4). Hashing is CPU- and memory-heavy, so every
//! operation runs on Tokio's blocking pool instead of an async worker thread.
//! Passwords and hashes are never logged.
//!
//! The number of Argon2 runs in flight is capped (one per CPU): each run
//! allocates `m_cost` KiB (about 19 MiB by default), and the blocking pool
//! would otherwise start up to 512 of them under a burst of sign-ups or
//! sign-ins. Extra requests wait for a permit instead; the request timeout
//! still bounds how long they wait.

use std::sync::Arc;

use argon2::password_hash::{self, PasswordHasher, PasswordVerifier};
use argon2::{Algorithm, Argon2, Params, Version};
use tokio::sync::{AcquireError, OwnedSemaphorePermit, Semaphore};

use crate::modules::auth::domain::PlainPassword;

/// Hashing or verification could not be carried out. A wrong password is not
/// an error: `verify` returns `Ok(false)` for it.
#[derive(Debug, thiserror::Error)]
pub enum HashError {
    /// Argon2 failed, or a stored hash is not a valid PHC string.
    #[error("password hashing failed: {0}")]
    Argon2(#[from] password_hash::Error),
    /// The blocking task panicked or was cancelled.
    #[error("password hashing task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
    /// The concurrency limiter was closed (never happens while the service lives).
    #[error("password hashing limiter closed: {0}")]
    Closed(#[from] AcquireError),
}

/// Hashes and verifies passwords with Argon2id v19. Clones share one
/// concurrency limit.
#[derive(Clone)]
pub struct PasswordHashService {
    params: Params,
    /// A PHC hash of a throw-away password, made with `params`, so that
    /// `verify_dummy` costs the same as verifying a real account's hash.
    dummy_hash: String,
    /// Caps concurrent Argon2 runs; see the module docs.
    permits: Arc<Semaphore>,
}

/// Password and salt of the dummy hash. Not secret: no account ever has it.
const DUMMY_PASSWORD: &[u8] = b"datenaow-dummy-password-never-used";
const DUMMY_SALT: &[u8] = b"datenaow-dummy-salt";

impl PasswordHashService {
    /// Builds the service and precomputes the dummy hash, which costs one
    /// Argon2 run at startup.
    ///
    /// # Panics
    ///
    /// Only if Argon2 cannot hash with already-validated `params` and a fixed
    /// valid salt, which does not happen short of memory exhaustion. Never
    /// called on a request path.
    pub fn new(params: Params) -> Self {
        let dummy_hash = argon2id(&params)
            .hash_password_with_salt(DUMMY_PASSWORD, DUMMY_SALT)
            .expect("Argon2 hashes a fixed password with validated params")
            .to_string();
        let max_concurrent = std::thread::available_parallelism().map_or(1, usize::from);
        Self {
            params,
            dummy_hash,
            permits: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Waits for a slot. The permit is moved into the blocking task, so it is
    /// held until Argon2 actually finishes, even if the request is dropped.
    async fn permit(&self) -> Result<OwnedSemaphorePermit, HashError> {
        Ok(self.permits.clone().acquire_owned().await?)
    }

    /// Hashes `p` with a fresh random salt and returns the PHC string
    /// (`$argon2id$v=19$m=...,t=...,p=...$salt$hash`).
    pub async fn hash(&self, p: &PlainPassword) -> Result<String, HashError> {
        let params = self.params.clone();
        let password = p.expose().to_owned();
        let permit = self.permit().await?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            argon2id(&params)
                .hash_password(password.as_bytes())
                .map(|hash| hash.to_string())
        })
        .await?
        .map_err(HashError::from)
    }

    /// Checks `p` against a stored PHC hash, using the parameters recorded in
    /// that hash. A wrong password gives `Ok(false)`; a malformed hash is an error.
    pub async fn verify(&self, p: &PlainPassword, phc_hash: &str) -> Result<bool, HashError> {
        let params = self.params.clone();
        let password = p.expose().to_owned();
        let phc_hash = phc_hash.to_owned();
        let permit = self.permit().await?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            match argon2id(&params).verify_password(password.as_bytes(), phc_hash.as_str()) {
                Ok(()) => Ok(true),
                Err(password_hash::Error::PasswordInvalid) => Ok(false),
                Err(e) => Err(HashError::from(e)),
            }
        })
        .await?
    }

    /// Verifies `p` against the dummy hash and discards the result, so that a
    /// sign-in for an unknown email takes as long as one for a real account.
    /// A failure is logged (without the password) and otherwise ignored,
    /// because the caller answers "invalid credentials" either way.
    pub async fn verify_dummy(&self, p: &PlainPassword) {
        if let Err(e) = self.verify(p, &self.dummy_hash).await {
            tracing::error!(error = %e, "dummy password verification failed");
        }
    }
}

fn argon2id(params: &Params) -> Argon2<'static> {
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Small parameters so the tests run fast (the minimum Argon2 allows).
    fn service() -> PasswordHashService {
        PasswordHashService::new(Params::new(1024, 1, 1, None).expect("valid test params"))
    }

    fn password(raw: &str) -> PlainPassword {
        PlainPassword::parse(raw.to_owned()).expect("valid password")
    }

    #[tokio::test]
    async fn hash_then_verify_returns_true() {
        let svc = service();
        let p = password("correct horse battery");
        let h = svc.hash(&p).await.expect("hash");
        assert!(svc.verify(&p, &h).await.expect("verify"));
    }

    #[tokio::test]
    async fn verify_wrong_password_returns_false() {
        let svc = service();
        let h = svc
            .hash(&password("correct horse battery"))
            .await
            .expect("hash");
        assert!(
            !svc.verify(&password("wrong horse battery"), &h)
                .await
                .expect("verify")
        );
    }

    #[tokio::test]
    async fn hash_is_argon2id_phc_string() {
        let h = service().hash(&password("mậtkhẩu😺")).await.expect("hash");
        assert!(h.starts_with("$argon2id$v=19$m=1024,t=1,p=1$"), "{h}");
    }

    #[tokio::test]
    async fn same_password_hashes_differently_because_of_random_salt() {
        let svc = service();
        let p = password("correct horse battery");
        assert_ne!(
            svc.hash(&p).await.expect("hash"),
            svc.hash(&p).await.expect("hash")
        );
    }

    #[tokio::test]
    async fn verify_malformed_hash_is_an_error() {
        let result = service()
            .verify(&password("correct horse battery"), "not-a-phc-hash")
            .await;
        assert!(matches!(result, Err(HashError::Argon2(_))));
    }

    #[tokio::test]
    async fn verify_uses_params_stored_in_the_hash() {
        // A hash made with other parameters (e.g. before a config change) still verifies.
        let old = PasswordHashService::new(Params::new(2048, 2, 1, None).expect("valid params"));
        let p = password("correct horse battery");
        let h = old.hash(&p).await.expect("hash");
        assert!(service().verify(&p, &h).await.expect("verify"));
    }

    #[tokio::test]
    async fn clones_share_one_limit_and_release_permits() {
        let svc = service();
        let clone = svc.clone();
        assert!(Arc::ptr_eq(&svc.permits, &clone.permits));

        let available = svc.permits.available_permits();
        assert!(available >= 1);
        let p = password("correct horse battery");
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..(available * 3) {
            let svc = svc.clone();
            let p = PlainPassword::parse(p.expose().to_owned()).expect("valid");
            set.spawn(async move { svc.hash(&p).await.is_ok() });
        }
        assert!(set.join_all().await.into_iter().all(|ok| ok));
        assert_eq!(svc.permits.available_permits(), available);
    }

    #[tokio::test]
    async fn dummy_hash_uses_configured_params() {
        let svc = service();
        assert!(svc.dummy_hash.starts_with("$argon2id$v=19$m=1024,t=1,p=1$"));
        svc.verify_dummy(&password("anything at all")).await;
    }
}
