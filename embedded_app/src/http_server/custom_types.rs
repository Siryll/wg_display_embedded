use alloc::string::String;
use picoserve::response::{IntoResponse, StatusCode};
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct Error(pub String);

impl Error {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

impl From<String> for Error {
    fn from(msg: String) -> Self {
        Self(msg)
    }
}

// returns HTTP 500 with message
impl IntoResponse for Error {
    async fn write_to<
        R: picoserve::io::Read,
        W: picoserve::response::ResponseWriter<Error = R::Error>,
    >(
        self,
        connection: picoserve::response::Connection<'_, R>,
        response_writer: W,
    ) -> Result<picoserve::ResponseSent, W::Error> {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            ("Content-Type", "text/plain"),
            self.0.as_str(),
        )
            .write_to(connection, response_writer)
            .await
    }
}

pub type HandlerResult<T> = Result<T, Error>;

pub struct HtmlResponse(pub String);

impl IntoResponse for HtmlResponse {
    async fn write_to<
        R: picoserve::io::Read,
        W: picoserve::response::ResponseWriter<Error = R::Error>,
    >(
        self,
        connection: picoserve::response::Connection<'_, R>,
        response_writer: W,
    ) -> Result<picoserve::ResponseSent, W::Error> {
        (
            ("Content-Type", "text/html; charset=utf-8"),
            self.0.as_str(),
        )
            .write_to(connection, response_writer)
            .await
    }
}

pub struct JsonStringResponse(pub String);

impl IntoResponse for JsonStringResponse {
    async fn write_to<
        R: picoserve::io::Read,
        W: picoserve::response::ResponseWriter<Error = R::Error>,
    >(
        self,
        connection: picoserve::response::Connection<'_, R>,
        response_writer: W,
    ) -> Result<picoserve::ResponseSent, W::Error> {
        (("Content-Type", "application/json"), self.0.as_str())
            .write_to(connection, response_writer)
            .await
    }
}

#[derive(Deserialize)]
pub struct ConfigWrapper {
    pub config: String,
}
