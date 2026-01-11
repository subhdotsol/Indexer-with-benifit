#[derive(Debug)]
pub enum AppError {
    InvalidSource,
    ParseError(String),
    DatabaseError(String),
}

pub type AppResult<T> = Result<T, AppError>;
