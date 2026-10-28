pub mod inference;
pub mod pixels;
pub mod protocol;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
