// Tanggung jawab: pub API crate sv-ir.
pub mod datatype;
pub mod design;
pub mod expr;
pub mod process;
pub mod scope;
pub mod system_task;
pub mod time_unit;
pub mod variable;

pub use datatype::{DataType, ValueKind};
pub use design::{Design, InstanceInfo, PortDirection, PortInfo};
pub use expr::{BinOp, Expr, SignalId, UnOp};
pub use process::{
    AssignStyle, Assignment, CaseArm, CaseKind, EdgeSensitivity, EventItem, Process, ProcessKind,
    SensitivityItem, Slice, Statement,
};
pub use scope::ScopePath;
pub use time_unit::TimeUnit;
pub use variable::{SignalTable, VarDecl, VarKind};
