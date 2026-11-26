use std::fmt;

/// Raw sign-up input; validated by `RegisterHandler`.
pub struct RegisterCommand {
    pub email: String,
    pub password: String,
}

/// Written by hand so the password and email never reach logs.
impl fmt::Debug for RegisterCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegisterCommand")
            .field("email", &"<redacted>")
            .field("password", &"***")
            .finish()
    }
}
