use uuid::Uuid;

/// `actor_id` comes from the verified access token, never from the client body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetCurrentUserQuery {
    pub actor_id: Uuid,
}
