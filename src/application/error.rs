use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Invalid source type provided")]
    InvalidSource,

    #[error("Error reading from gRPC stream")]
    GrpcStreamingError,

    #[error("Error fetching data from gRPC endpoint")]
    ErrorFetchingDataFromGrpc,

    #[error("Error parsing RPC response")]
    RPCParsingError,

    #[error("Channel send error: buffer closed or full")]
    ErrorSendingMessageViaBuffer,
}

pub type AppResult<T> = Result<T, AppError>;
