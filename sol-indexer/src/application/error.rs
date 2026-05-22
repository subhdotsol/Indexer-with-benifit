use thiserror::Error;

#[derive(Debug,Error)]
pub enum AppError{
    #[error("Invalid Source is provided")]
    InvalidSource,

    #[error("Error in getting data from stream")]
    GrpcStreamingError,

    #[error("Error fetching Data from Grpc")]
    ErrorFetchingDataFromGrpc,

    #[error("Error parsing rpc data")]
    RPCParsingError,

    #[error("Error Sending message via buffer")]
    ErrorSendingMessageViaBuffer,
}

pub type AppResult<T> = Result<T,AppError>;