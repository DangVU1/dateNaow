use std::fmt;

/// Raw reset-confirmation input; validated by `ConfirmPasswordResetHandler`.
pub struct ConfirmPasswordResetCommand {
    pub token: String,
    pub new_password: String,
}

/// Written by hand so the token and password never reach logs.
impl fmt::Debug for ConfirmPasswordResetCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfirmPasswordResetCommand")
            .field("token", &"***")
            .field("new_password", &"***")
            .finish()
    }
}
