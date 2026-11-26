//! Sessions: a short-lived access token (JWT) plus a long-lived, revocable
//! refresh token of which only the hash is stored (P3).

use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::modules::auth::application::error::AuthError;
use crate::modules::auth::application::services::jwt::JwtService;
use crate::modules::auth::domain::{OpaqueToken, RefreshRepoError, RefreshTokenRepository};
use crate::modules::users::domain::{User, UserRepository};
use crate::shared::clock::Clock;

/// What a successful sign-in, sign-up or refresh hands to the client.
pub struct Session {
    pub access_token: String,
    pub access_expires_in_secs: i64,
    /// Sent to the client once, in the refresh cookie; never stored.
    pub refresh_token: OpaqueToken,
    pub refresh_expires_at: DateTime<Utc>,
}

/// Written by hand so the access token never reaches logs.
impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("access_token", &"***")
            .field("access_expires_in_secs", &self.access_expires_in_secs)
            .field("refresh_token", &self.refresh_token)
            .field("refresh_expires_at", &self.refresh_expires_at)
            .finish()
    }
}

/// How long a just-rotated refresh token may still be presented without
/// counting as theft (P3). Within it the client gets `RefreshConflict`;
/// typically another tab refreshed at the same moment.
const ROTATION_GRACE: chrono::Duration = chrono::Duration::seconds(30);

pub struct SessionService<U: UserRepository, R: RefreshTokenRepository> {
    users: U,
    tokens: R,
    jwt: Arc<JwtService>,
    clock: Arc<dyn Clock>,
    refresh_ttl: chrono::Duration,
}

impl<U: UserRepository, R: RefreshTokenRepository> SessionService<U, R> {
    pub fn new(
        users: U,
        tokens: R,
        jwt: Arc<JwtService>,
        clock: Arc<dyn Clock>,
        refresh_ttl: chrono::Duration,
    ) -> Self {
        Self {
            users,
            tokens,
            jwt,
            clock,
            refresh_ttl,
        }
    }

    /// Starts a new session for `user`: issues an access token and stores the
    /// hash of a new refresh token that expires `refresh_ttl` from now.
    ///
    /// `password_hash` is the hash the caller just verified (sign-in) or
    /// stored (sign-up). If a password reset replaced it in the meantime, no
    /// token is stored and the result is `InvalidCredentials` (P6).
    pub async fn start(&self, user: &User, password_hash: &str) -> Result<Session, AuthError> {
        let now = self.clock.now();
        let refresh_token = OpaqueToken::generate().map_err(AuthError::internal)?;
        let refresh_expires_at = self.refresh_expiry(now)?;

        match self
            .tokens
            .insert(
                user.id,
                password_hash,
                refresh_token.hash(),
                refresh_expires_at,
            )
            .await
        {
            Ok(_) => {}
            Err(RefreshRepoError::CredentialsChanged) => {
                return Err(AuthError::InvalidCredentials);
            }
            Err(e) => return Err(AuthError::internal(e)),
        }

        self.session(user, refresh_token, refresh_expires_at, now)
    }

    /// Exchanges the presented refresh token for a new session (P3). The
    /// checks run in this order:
    ///
    /// 1. malformed or unknown token: `InvalidRefreshToken`;
    /// 2. expired (`now >= expires_at`): `InvalidRefreshToken`;
    /// 3. rotated at most 30 seconds ago: `RefreshConflict`, nothing revoked;
    /// 4. rotated more than 30 seconds ago: reuse of a stolen token, so every
    ///    session of the user is revoked, then `InvalidRefreshToken`;
    /// 5. revoked otherwise (logout, password reset, reuse detection):
    ///    `InvalidRefreshToken`, nothing else revoked;
    /// 6. valid: rotated to a new token expiring 30 days from now; losing a
    ///    concurrent rotation gives `RefreshConflict`;
    /// 7. the user is reloaded so the access token carries the current role;
    ///    a deleted account gives `InvalidRefreshToken`. This load runs
    ///    before the rotation of step 6, so nothing fallible but JWT signing
    ///    happens after the rotation commits.
    pub async fn refresh(&self, presented: &str) -> Result<(User, Session), AuthError> {
        // 1. Lookups go by hash only; the token itself is never compared.
        let presented =
            OpaqueToken::from_client(presented).map_err(|_| AuthError::InvalidRefreshToken)?;
        let record = self
            .tokens
            .find_by_hash(presented.hash())
            .await
            .map_err(AuthError::internal)?
            .ok_or(AuthError::InvalidRefreshToken)?;
        let now = self.clock.now();

        // 2.
        if now >= record.expires_at {
            return Err(AuthError::InvalidRefreshToken);
        }

        // 3, 4, 5.
        if record.revoked_at.is_some() {
            return match record.replaced_at {
                Some(replaced_at) if now - replaced_at <= ROTATION_GRACE => {
                    Err(AuthError::RefreshConflict)
                }
                Some(_) => {
                    // Logged first so the event is recorded even if the
                    // revocation below fails.
                    tracing::warn!(user_id = %record.user_id, "refresh token reuse detected");
                    self.tokens
                        .revoke_all_for_user(record.user_id, now)
                        .await
                        .map_err(AuthError::internal)?;
                    Err(AuthError::InvalidRefreshToken)
                }
                None => Err(AuthError::InvalidRefreshToken),
            };
        }

        // 7 (done before 6): the user is loaded before rotating, so a failure
        // here leaves the presented token untouched. Rotating first and then
        // failing would strand the client with a replaced token, and a retry
        // after 30 s would look like theft and revoke every session.
        let user = self
            .users
            .find_by_id(record.user_id)
            .await
            .map_err(AuthError::internal)?
            .ok_or(AuthError::InvalidRefreshToken)?;

        // 6.
        let new_token = OpaqueToken::generate().map_err(AuthError::internal)?;
        let new_expires_at = self.refresh_expiry(now)?;
        match self
            .tokens
            .rotate(
                record.id,
                record.user_id,
                new_token.hash(),
                new_expires_at,
                now,
            )
            .await
        {
            Ok(_) => {}
            Err(RefreshRepoError::AlreadyRevoked) => return Err(AuthError::RefreshConflict),
            Err(e) => return Err(AuthError::internal(e)),
        }

        let session = self.session(&user, new_token, new_expires_at, now)?;
        Ok((user, session))
    }

    /// Ends the session of the presented refresh token. A malformed, unknown
    /// or already revoked token is ignored.
    pub async fn end(&self, presented: &str) -> Result<(), AuthError> {
        let Ok(presented) = OpaqueToken::from_client(presented) else {
            return Ok(());
        };
        let Some(record) = self
            .tokens
            .find_by_hash(presented.hash())
            .await
            .map_err(AuthError::internal)?
        else {
            return Ok(());
        };
        self.tokens
            .revoke(record.id, self.clock.now())
            .await
            .map_err(AuthError::internal)
    }

    /// Issues the access token and bundles it with an already stored refresh token.
    fn session(
        &self,
        user: &User,
        refresh_token: OpaqueToken,
        refresh_expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Session, AuthError> {
        let access_token = self
            .jwt
            .issue(user.id, user.role, now)
            .map_err(AuthError::internal)?;
        Ok(Session {
            access_token,
            access_expires_in_secs: self.jwt.access_ttl_secs(),
            refresh_token,
            refresh_expires_at,
        })
    }

    fn refresh_expiry(&self, now: DateTime<Utc>) -> Result<DateTime<Utc>, AuthError> {
        now.checked_add_signed(self.refresh_ttl)
            .ok_or_else(|| AuthError::Internal("refresh token expiry out of range".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use chrono::Duration;
    use uuid::Uuid;

    use super::*;
    use crate::modules::auth::domain::RefreshTokenRecord;
    use crate::modules::users::domain::{Email, Gender, NewUser, UserCredentials, UserRepoError};
    use crate::shared::actor::Role;

    const USER_ID: Uuid = Uuid::from_u128(1);
    const TOKEN_ID: Uuid = Uuid::from_u128(42);

    fn t0() -> DateTime<Utc> {
        "2026-11-25T09:00:00Z".parse().unwrap()
    }

    fn user(role: Role) -> User {
        User {
            id: USER_ID,
            email: Email::parse("an@example.com").unwrap(),
            role,
            gender: None,
            created_at: t0(),
        }
    }

    #[derive(Default)]
    struct Users(Mutex<Option<User>>);

    impl UserRepository for Arc<Users> {
        async fn create(&self, _: NewUser) -> Result<User, UserRepoError> {
            unimplemented!("not used by SessionService")
        }

        async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, UserRepoError> {
            Ok(self.0.lock().unwrap().clone().filter(|u| u.id == id))
        }

        async fn find_credentials_by_email(
            &self,
            _: &Email,
        ) -> Result<Option<UserCredentials>, UserRepoError> {
            unimplemented!("not used by SessionService")
        }

        async fn update_gender(&self, _: Uuid, _: Option<Gender>) -> Result<User, UserRepoError> {
            unimplemented!("not used by SessionService")
        }

        async fn set_role(&self, _: &Email, _: Role) -> Result<bool, UserRepoError> {
            unimplemented!("not used by SessionService")
        }
    }

    /// In-memory tokens keyed by hash, recording every call that changes state.
    #[derive(Default)]
    struct Tokens {
        rows: Mutex<Vec<([u8; 32], RefreshTokenRecord)>>,
        /// `(old_id, new_expires_at)` per successful rotation.
        rotated: Mutex<Vec<(Uuid, DateTime<Utc>)>>,
        revoked: Mutex<Vec<Uuid>>,
        revoked_all: Mutex<Vec<Uuid>>,
        /// Makes `rotate` lose a simulated race with a concurrent request.
        rotate_loses_race: AtomicBool,
    }

    impl RefreshTokenRepository for Arc<Tokens> {
        async fn insert(
            &self,
            user_id: Uuid,
            _: &str,
            token_hash: [u8; 32],
            expires_at: DateTime<Utc>,
        ) -> Result<RefreshTokenRecord, RefreshRepoError> {
            let record = RefreshTokenRecord {
                id: Uuid::new_v4(),
                user_id,
                expires_at,
                revoked_at: None,
                replaced_at: None,
            };
            self.rows.lock().unwrap().push((token_hash, record.clone()));
            Ok(record)
        }

        async fn find_by_hash(
            &self,
            token_hash: [u8; 32],
        ) -> Result<Option<RefreshTokenRecord>, RefreshRepoError> {
            let rows = self.rows.lock().unwrap();
            Ok(rows
                .iter()
                .find(|(hash, _)| *hash == token_hash)
                .map(|(_, record)| record.clone()))
        }

        async fn rotate(
            &self,
            old_id: Uuid,
            user_id: Uuid,
            new_hash: [u8; 32],
            new_expires_at: DateTime<Utc>,
            now: DateTime<Utc>,
        ) -> Result<RefreshTokenRecord, RefreshRepoError> {
            if self.rotate_loses_race.load(Ordering::Relaxed) {
                return Err(RefreshRepoError::AlreadyRevoked);
            }
            self.rotated.lock().unwrap().push((old_id, new_expires_at));
            for (_, record) in self.rows.lock().unwrap().iter_mut() {
                if record.id == old_id {
                    record.revoked_at = Some(now);
                    record.replaced_at = Some(now);
                }
            }
            self.insert(user_id, "", new_hash, new_expires_at).await
        }

        async fn revoke(&self, id: Uuid, _: DateTime<Utc>) -> Result<(), RefreshRepoError> {
            self.revoked.lock().unwrap().push(id);
            Ok(())
        }

        async fn revoke_all_for_user(
            &self,
            user_id: Uuid,
            _: DateTime<Utc>,
        ) -> Result<u64, RefreshRepoError> {
            self.revoked_all.lock().unwrap().push(user_id);
            Ok(1)
        }
    }

    struct FixedClock(DateTime<Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    struct Fixture {
        service: SessionService<Arc<Users>, Arc<Tokens>>,
        users: Arc<Users>,
        tokens: Arc<Tokens>,
        jwt: Arc<JwtService>,
    }

    /// A service whose clock is fixed at `t0()`, with the user (role `user`) present.
    fn fixture() -> Fixture {
        let users = Arc::new(Users(Mutex::new(Some(user(Role::User)))));
        let tokens = Arc::new(Tokens::default());
        let jwt = Arc::new(JwtService::new(
            b"unit-test-secret-that-is-at-least-32-bytes",
            Duration::seconds(900),
        ));
        let service = SessionService::new(
            users.clone(),
            tokens.clone(),
            jwt.clone(),
            Arc::new(FixedClock(t0())),
            Duration::days(30),
        );
        Fixture {
            service,
            users,
            tokens,
            jwt,
        }
    }

    impl Fixture {
        /// Stores a token of the user with id `TOKEN_ID` and returns it.
        fn stored(
            &self,
            expires_at: DateTime<Utc>,
            revoked_at: Option<DateTime<Utc>>,
            replaced_at: Option<DateTime<Utc>>,
        ) -> OpaqueToken {
            let token = OpaqueToken::generate().unwrap();
            let record = RefreshTokenRecord {
                id: TOKEN_ID,
                user_id: USER_ID,
                expires_at,
                revoked_at,
                replaced_at,
            };
            self.tokens
                .rows
                .lock()
                .unwrap()
                .push((token.hash(), record));
            token
        }

        /// A token rotated `ago` before `t0()` that would otherwise be valid.
        fn rotated(&self, ago: Duration) -> OpaqueToken {
            let replaced = t0() - ago;
            self.stored(t0() + Duration::days(1), Some(replaced), Some(replaced))
        }

        fn nothing_changed(&self) -> bool {
            self.tokens.rotated.lock().unwrap().is_empty()
                && self.tokens.revoked.lock().unwrap().is_empty()
                && self.tokens.revoked_all.lock().unwrap().is_empty()
        }
    }

    #[tokio::test]
    async fn malformed_or_unknown_token_is_invalid() {
        let f = fixture();
        let upper = "AB".repeat(32);
        let unknown = "ab".repeat(32);
        for presented in ["", "nope", upper.as_str(), unknown.as_str()] {
            let res = f.service.refresh(presented).await;
            assert!(
                matches!(res, Err(AuthError::InvalidRefreshToken)),
                "{presented}"
            );
        }
        assert!(f.nothing_changed());
    }

    #[tokio::test]
    async fn token_is_expired_from_expires_at_on() {
        let f = fixture();
        let at_expiry = f.stored(t0(), None, None);
        let res = f.service.refresh(at_expiry.as_str()).await;
        assert!(matches!(res, Err(AuthError::InvalidRefreshToken)));
        assert!(f.nothing_changed());

        let f = fixture();
        let just_before = f.stored(t0() + Duration::milliseconds(1), None, None);
        assert!(f.service.refresh(just_before.as_str()).await.is_ok());
    }

    #[tokio::test]
    async fn expiry_is_checked_before_rotation_rules() {
        // Rotated 10 s ago (inside the grace period) but expired: 401, not 409.
        let f = fixture();
        let replaced = t0() - Duration::seconds(10);
        let token = f.stored(t0(), Some(replaced), Some(replaced));
        let res = f.service.refresh(token.as_str()).await;
        assert!(matches!(res, Err(AuthError::InvalidRefreshToken)));

        // Rotated long ago and expired: no reuse revocation either.
        let replaced = t0() - Duration::days(1);
        let token = f.stored(t0(), Some(replaced), Some(replaced));
        let res = f.service.refresh(token.as_str()).await;
        assert!(matches!(res, Err(AuthError::InvalidRefreshToken)));
        assert!(f.nothing_changed());
    }

    #[tokio::test]
    async fn rotated_exactly_30s_ago_is_a_conflict_and_revokes_nothing() {
        let f = fixture();
        let token = f.rotated(Duration::seconds(30));

        let res = f.service.refresh(token.as_str()).await;

        assert!(matches!(res, Err(AuthError::RefreshConflict)));
        assert!(f.nothing_changed());
    }

    #[tokio::test]
    async fn rotated_just_over_30s_ago_revokes_all_sessions() {
        let f = fixture();
        let token = f.rotated(Duration::seconds(30) + Duration::milliseconds(1));

        let res = f.service.refresh(token.as_str()).await;

        assert!(matches!(res, Err(AuthError::InvalidRefreshToken)));
        assert_eq!(*f.tokens.revoked_all.lock().unwrap(), [USER_ID]);
        assert!(f.tokens.rotated.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn revoked_without_replacement_is_invalid_without_cascade() {
        let f = fixture();
        let revoked = t0() - Duration::days(1);
        let token = f.stored(t0() + Duration::days(1), Some(revoked), None);

        let res = f.service.refresh(token.as_str()).await;

        assert!(matches!(res, Err(AuthError::InvalidRefreshToken)));
        assert!(f.nothing_changed());
    }

    #[tokio::test]
    async fn losing_a_concurrent_rotation_is_a_conflict() {
        let f = fixture();
        f.tokens.rotate_loses_race.store(true, Ordering::Relaxed);
        let token = f.stored(t0() + Duration::days(1), None, None);

        let res = f.service.refresh(token.as_str()).await;

        assert!(matches!(res, Err(AuthError::RefreshConflict)));
        assert!(f.nothing_changed());
    }

    #[tokio::test]
    async fn valid_token_rotates_to_30_days_from_now_with_current_role() {
        let f = fixture();
        *f.users.0.lock().unwrap() = Some(user(Role::Admin));
        let token = f.stored(t0() + Duration::hours(1), None, None);

        let (user, session) = f.service.refresh(token.as_str()).await.unwrap();

        assert_eq!(user.role, Role::Admin);
        assert_eq!(session.refresh_expires_at, t0() + Duration::days(30));
        assert_ne!(session.refresh_token.hash(), token.hash());
        assert_eq!(
            *f.tokens.rotated.lock().unwrap(),
            [(TOKEN_ID, t0() + Duration::days(30))]
        );
        let claims = f.jwt.verify(&session.access_token, t0()).unwrap();
        assert_eq!(claims.sub, USER_ID);
        assert_eq!(claims.role, Role::Admin);
    }

    #[tokio::test]
    async fn deleted_user_is_invalid() {
        let f = fixture();
        *f.users.0.lock().unwrap() = None;
        let token = f.stored(t0() + Duration::hours(1), None, None);

        let res = f.service.refresh(token.as_str()).await;

        assert!(matches!(res, Err(AuthError::InvalidRefreshToken)));
        // The user is loaded before rotating, so a failed load rotates nothing.
        assert!(f.nothing_changed());
    }

    #[tokio::test]
    async fn end_revokes_a_known_token_and_ignores_others() {
        let f = fixture();
        f.service.end("garbage").await.unwrap();
        f.service.end(&"ab".repeat(32)).await.unwrap();
        assert!(f.nothing_changed());

        let token = f.stored(t0() + Duration::hours(1), None, None);
        f.service.end(token.as_str()).await.unwrap();
        assert_eq!(*f.tokens.revoked.lock().unwrap(), [TOKEN_ID]);
        assert!(f.tokens.revoked_all.lock().unwrap().is_empty());
    }
}
