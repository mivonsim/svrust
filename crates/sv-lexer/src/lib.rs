// Tanggung jawab: pub API crate sv-lexer.
pub mod error;
pub mod keyword;
pub mod lexer;
pub mod literal;
pub mod operator;
pub mod skip;
pub mod span;
pub mod spanned;
pub mod system_task;
pub mod token;

pub use error::LexError;
pub use span::Span;
pub use spanned::SpannedToken;
pub use token::Token;

pub use lexer::lex;
