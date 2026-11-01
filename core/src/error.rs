use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// A message that is already phrased for the user.
    Msg(String),
    Http(reqwest::Error),
    Io(std::io::Error),
    Json(serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn msg(s: impl Into<String>) -> Self {
        Error::Msg(s.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Msg(m) => write!(f, "{m}"),
            Error::Http(e) if e.is_timeout() => write!(f, "The server took too long to answer."),
            Error::Http(e) if e.is_connect() => write!(f, "Could not connect to the server. Check your internet connection."),
            Error::Http(e) => write!(f, "Network problem: {e}"),
            Error::Io(e) => write!(f, "File problem: {e}"),
            Error::Json(e) => write!(f, "Unexpected data from the server: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Http(e)
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}
