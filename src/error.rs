use thiserror::Error;
use tonic::Status;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("beatmap source required")]
    MissingBeatmap,

    #[error("parse error: {0}")]
    Parse(std::io::Error),

    #[error("conversion not supported")]
    Conversion,

    #[error("suspicious beatmap: {0:?}")]
    Suspicious(rosu_pp::model::beatmap::TooSuspicious),

    #[error("batch too large: {0} requests, max {1}")]
    BatchTooLarge(usize, usize),

    #[error("no strain data available")]
    NoStrainData,

    #[error("graph rendering failed: {0}")]
    GraphRendering(String),
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Parse(err)
    }
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        match &err {
            Error::MissingBeatmap | Error::BatchTooLarge(_, _) => {
                Status::invalid_argument(err.to_string())
            }
            Error::Parse(_) | Error::Suspicious(_) => Status::failed_precondition(err.to_string()),
            Error::Conversion => Status::unimplemented(err.to_string()),
            Error::NoStrainData => Status::failed_precondition(err.to_string()),
            Error::GraphRendering(_) => Status::internal(err.to_string()),
        }
    }
}
