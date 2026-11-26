use std::fmt;
use std::net::IpAddr;

/// Raw reset-request input. `client_ip` comes from the connection (or, behind
/// a trusted proxy, `X-Forwarded-For`), never from the body.
pub struct RequestPasswordResetCommand {
    pub email: String,
    pub client_ip: IpAddr,
}

/// Written by hand so the email never reaches logs.
impl fmt::Debug for RequestPasswordResetCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestPasswordResetCommand")
            .field("email", &"<redacted>")
            .field("client_ip", &self.client_ip)
            .finish()
    }
}
