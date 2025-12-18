use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    #[error(transparent)]
    SignalsError(#[from] ctrlc::Error),
}
