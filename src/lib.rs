pub mod inference;
mod latex;
pub mod pixels;
pub mod protocol;
pub mod session;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
