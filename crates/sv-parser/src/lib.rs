// Tanggung jawab: pub API crate sv-parser.
pub mod always_comb;
pub mod always_ff;
pub mod combinational;
pub mod declaration;
pub mod dimension;
pub mod event_control;
pub mod expression;
pub mod file;
pub mod generate;
pub mod initial;
pub mod instance;
pub mod local_scope;
pub mod lvalue;
pub mod module;
pub mod param;
pub mod port;
pub mod routine;
pub mod statement;
pub mod typedef;

pub use dimension::parse_dimension;
pub use file::parse_file;
pub use lvalue::parse_lvalue;
pub use module::parse_module;
