use sqlx::PgPool;
use uuid::Uuid;

use datenaow::modules::users::domain::{Email, Gender, NewUser, UserRepoError, UserRepository};
use datenaow::modules::users::infrastructure::PgUserRepository;
use datenaow::shared::actor::Role;

fn email(raw: &str) -> Email {
    Email::parse(raw).expect("valid test email")
}

fn new_user(raw_email: &str, hash: &str) -> NewUser {
    NewUser {
        email: email(raw_email),
        password_hash: hash.to_owned(),
    }
}

async fn stored_hash(pool: &PgPool, id: Uuid) -> String {
    sqlx::query_scalar::<_, String>("SELECT password_hash FROM users WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("read stored hash")
}

#[sqlx::test]
async fn create_then_find_by_id_returns_same_user(pool: PgPool) {
    let repo = PgUserRepository::new(pool);

    let created = repo
        .create(new_user("  An@Example.COM ", "hash-1"))
        .await
        .expect("create user");
    let found = repo
        .find_by_id(created.id)
        .await
        .expect("find user")
        .expect("user exists");

    assert_eq!(found, created);
    assert_eq!(found.email.as_str(), "an@example.com");
    assert_eq!(found.role, Role::User);
    assert_eq!(found.gender, None);
}

#[sqlx::test]
async fn find_by_id_for_unknown_id_returns_none(pool: PgPool) {
    let repo = PgUserRepository::new(pool);
    let res = repo.find_by_id(Uuid::nil()).await.expect("query runs");
    assert!(res.is_none());
}

#[sqlx::test]
async fn create_with_existing_email_returns_email_taken(pool: PgPool) {
    let repo = PgUserRepository::new(pool);
    repo.create(new_user("an@example.com", "hash-1"))
        .await
        .expect("first create");

    // Same address with different case and surrounding spaces (Review Focus #2).
    let err = repo
        .create(new_user(" AN@example.com", "hash-2"))
        .await
        .expect_err("duplicate email must fail");

    assert!(matches!(err, UserRepoError::EmailTaken), "{err:?}");
}

#[sqlx::test]
async fn find_credentials_by_email_returns_hash_and_role(pool: PgPool) {
    let repo = PgUserRepository::new(pool);
    let created = repo
        .create(new_user("an@example.com", "hash-1"))
        .await
        .expect("create user");

    let creds = repo
        .find_credentials_by_email(&email("AN@Example.com"))
        .await
        .expect("query runs")
        .expect("credentials exist");

    assert_eq!(creds.user_id, created.id);
    assert_eq!(creds.password_hash, "hash-1");
    assert_eq!(creds.role, Role::User);
}

#[sqlx::test]
async fn find_credentials_for_unknown_email_returns_none(pool: PgPool) {
    let repo = PgUserRepository::new(pool);
    let res = repo
        .find_credentials_by_email(&email("nobody@example.com"))
        .await
        .expect("query runs");
    assert!(res.is_none());
}

#[sqlx::test]
async fn update_gender_sets_then_clears(pool: PgPool) {
    let repo = PgUserRepository::new(pool);
    let created = repo
        .create(new_user("an@example.com", "hash-1"))
        .await
        .expect("create user");

    let u1 = repo
        .update_gender(created.id, Some(Gender::Female))
        .await
        .expect("set gender");
    let u2 = repo
        .update_gender(created.id, None)
        .await
        .expect("clear gender");

    assert_eq!(u1.gender, Some(Gender::Female));
    assert_eq!(u2.gender, None);
    assert_eq!(u2.id, created.id);
}

#[sqlx::test]
async fn update_gender_for_missing_user_returns_not_found(pool: PgPool) {
    let repo = PgUserRepository::new(pool);
    let err = repo
        .update_gender(Uuid::nil(), Some(Gender::Male))
        .await
        .expect_err("missing user must fail");
    assert!(matches!(err, UserRepoError::NotFound), "{err:?}");
}

#[sqlx::test]
async fn set_role_promotes_existing_user_and_reports_missing(pool: PgPool) {
    let repo = PgUserRepository::new(pool);
    let created = repo
        .create(new_user("an@example.com", "hash-1"))
        .await
        .expect("create user");

    let promoted = repo
        .set_role(&email("an@example.com"), Role::Admin)
        .await
        .expect("set role");
    let missing = repo
        .set_role(&email("nobody@example.com"), Role::Admin)
        .await
        .expect("set role on missing");

    assert!(promoted);
    assert!(!missing);
    let found = repo
        .find_by_id(created.id)
        .await
        .expect("find user")
        .expect("user exists");
    assert_eq!(found.role, Role::Admin);
}

#[sqlx::test]
async fn set_password_hash_in_tx_is_rolled_back_with_transaction(pool: PgPool) {
    let repo = PgUserRepository::new(pool.clone());
    let created = repo
        .create(new_user("an@example.com", "hash-old"))
        .await
        .expect("create user");

    let mut tx = pool.begin().await.expect("begin");
    PgUserRepository::set_password_hash_in_tx(&mut tx, created.id, "hash-new")
        .await
        .expect("set hash in tx");
    tx.rollback().await.expect("rollback");

    assert_eq!(stored_hash(&pool, created.id).await, "hash-old");
}

#[sqlx::test]
async fn set_password_hash_in_tx_is_kept_after_commit(pool: PgPool) {
    let repo = PgUserRepository::new(pool.clone());
    let created = repo
        .create(new_user("an@example.com", "hash-old"))
        .await
        .expect("create user");

    let mut tx = pool.begin().await.expect("begin");
    PgUserRepository::set_password_hash_in_tx(&mut tx, created.id, "hash-new")
        .await
        .expect("set hash in tx");
    tx.commit().await.expect("commit");

    assert_eq!(stored_hash(&pool, created.id).await, "hash-new");
}

#[sqlx::test]
async fn set_password_hash_in_tx_for_missing_user_returns_not_found(pool: PgPool) {
    let mut conn = pool.acquire().await.expect("acquire");
    let err = PgUserRepository::set_password_hash_in_tx(&mut conn, Uuid::nil(), "hash")
        .await
        .expect_err("missing user must fail");
    assert!(matches!(err, UserRepoError::NotFound), "{err:?}");
}
