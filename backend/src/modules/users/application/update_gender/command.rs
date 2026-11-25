use uuid::Uuid;

/// `gender: None` clears the choice. `actor_id` comes from the verified
/// access token, never from the client body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateGenderCommand {
    pub actor_id: Uuid,
    pub gender: Option<String>,
}
