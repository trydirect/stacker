mod f_agent;
mod f_anonym;
mod f_cookie;
mod f_hmac;
mod f_oauth;

pub use f_agent::try_agent;
pub use f_anonym::anonym;
pub use f_cookie::try_cookie;
pub use f_hmac::try_hmac;
pub use f_oauth::try_oauth;
