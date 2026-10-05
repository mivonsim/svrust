// Tanggung jawab: pub API crate sv-elaborator.
pub mod design;
pub mod error;
pub mod generate;
pub mod generate_proses;
pub mod instance;
pub mod konst;
pub mod local_decl;
pub mod lower_expr;
pub mod lower_stmt;
pub mod param;
pub mod routine;
pub mod routine_subst;
pub mod symbol;
pub mod typedef;
pub mod typedef_walk;
pub mod width;

pub use design::{elaborate, elaborate_top, DesignStats};
pub use error::ElaborateError;
pub use symbol::{Symbol, SymbolTable};
pub use typedef::{TypeInfo, TypeTable};
pub use width::{infer_binary, infer_ternary, resize_action, Context, ResizeAction};
