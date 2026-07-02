//! Reusable row-mapping helpers for the OxiSQL-backed storage layer.
//!
//! Every store maps an [`oxisql_core::Row`] into a typed row struct through the
//! single [`row_to!`] macro defined here, replacing the old
//! `#[derive(sqlx::FromRow)]` + `query_as::<_, T>(..)` idiom. Defining the
//! mapping in one place keeps column-name extraction consistent across all
//! store modules and avoids bespoke per-file row-decoding code.

use oxisql_core::{OxiSqlError, Row};

/// Extension trait adding a concise, type-inferred column accessor to
/// [`oxisql_core::Row`].
pub trait RowExt {
    /// Extract the value of column `name`, converting it to `T` via
    /// [`oxisql_core::FromValue`].
    fn col<T: oxisql_core::FromValue>(&self, name: &str) -> Result<T, OxiSqlError>;
}

impl RowExt for Row {
    fn col<T: oxisql_core::FromValue>(&self, name: &str) -> Result<T, OxiSqlError> {
        self.try_get(name)
    }
}

/// Build a closure `FnMut(&Row) -> Result<Ty, OxiSqlError>` that maps a row's
/// named columns onto the fields of `Ty`.
///
/// Usage mirrors the shape of the target struct:
///
/// ```ignore
/// let workflows: Vec<WorkflowRow> = rows
///     .iter()
///     .map(row_to!(WorkflowRow { id: "id", name: "name" }))
///     .collect::<Result<_, _>>()?;
/// ```
macro_rules! row_to {
    ($ty:ident { $($field:ident : $col:literal),+ $(,)? }) => {
        |row: &::oxisql_core::Row| -> ::std::result::Result<$ty, ::oxisql_core::OxiSqlError> {
            use $crate::row_ext::RowExt;
            Ok($ty { $( $field: row.col($col)?, )+ })
        }
    };
}
pub(crate) use row_to;
