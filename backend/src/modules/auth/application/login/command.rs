use std::fmt;
use std::net::IpAddr;

/// Raw sign-in input. `client_ip` comes from the connection (or, behind a
/// trusted proxy, `X-Forwarded-For`), never from the body.
pub struct LoginCommand {
    pub email: String,
    pub password: String,
    pub client_ip: IpAddr,
}

/// Written by hand so the password and email never reach logs.
impl fmt::Debug for LoginCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoginCommand")
            .field("email", &"<redacted>")
            .field("password", &"***")
            .field("client_ip", &self.client_ip)
            .finish()
    }
}
