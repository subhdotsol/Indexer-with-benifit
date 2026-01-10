#[derive(Debug)]
pub enum AppError{
    InvalidSource,
}

pub type AppResult<T> = Result<T,AppError>;
