use std::fmt;
use std::net::IpAddr;

/// Raw reset-confirmation input; validated by `ConfirmPasswordResetHandler`.
/// `client_ip` comes from the connection (or, behind a trusted proxy, from
/// `X-Forwarded-For`) and selects the sign-in pair counter to clear.
pub struct ConfirmPasswordResetCommand {
    pub token: String,
    pub new_password: String,
    pub client_ip: IpAddr,
}

/// Written by hand so the token and password never reach logs.
impl fmt::Debug for ConfirmPasswordResetCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfirmPasswordResetCommand")
            .field("token", &"***")
            .field("new_password", &"***")
            .field("client_ip", &self.client_ip)
            .finish()
    }
}
