//! SQL Query Builder Utilities
//!
//! Provides a type-safe, fluent API for constructing dynamic SQL queries.
//!
//! ## Overview
//!
//! The query builder helps construct complex SQL queries programmatically while maintaining
//! type safety and preventing SQL injection through parameter binding.
//!
//! ## Features
//!
//! - **Fluent API**: Chain methods to build queries naturally
//! - **Type Safety**: Compile-time checks for query structure
//! - **Parameter Binding**: Automatic SQL injection protection
//! - **Dynamic Filters**: Conditionally add WHERE clauses
//! - **Sorting**: Type-safe ORDER BY clauses
//! - **Pagination**: LIMIT and OFFSET support
//! - **Aggregations**: COUNT, SUM, AVG, MIN, MAX support
//!
//! ## Example
//!
//! ```rust
//! use oxify_storage::query_builder::{QueryBuilder, Condition, SortOrder};
//!
//! let mut builder = QueryBuilder::new("workflows")
//!     .select(&["id", "name", "created_at"])
//!     .where_condition(Condition::Eq("user_id".to_string()))
//!     .where_condition(Condition::IsNotNull("deleted_at".to_string()))
//!     .order_by("created_at", SortOrder::Desc)
//!     .limit(20)
//!     .offset(0);
//!
//! let (sql, param_count) = builder.build();
//! // SELECT id, name, created_at FROM workflows
//! // WHERE user_id = $1 AND deleted_at IS NOT NULL
//! // ORDER BY created_at DESC LIMIT 20 OFFSET 0
//! ```

use std::fmt;

/// Sort order for ORDER BY clauses
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    /// Ascending order (smallest to largest)
    Asc,
    /// Descending order (largest to smallest)
    Desc,
}

impl fmt::Display for SortOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SortOrder::Asc => write!(f, "ASC"),
            SortOrder::Desc => write!(f, "DESC"),
        }
    }
}

/// SQL condition types for WHERE clauses
#[derive(Debug, Clone)]
pub enum Condition {
    /// column = $n
    Eq(String),
    /// column != $n
    NotEq(String),
    /// column > $n
    Gt(String),
    /// column >= $n
    Gte(String),
    /// column < $n
    Lt(String),
    /// column <= $n
    Lte(String),
    /// column LIKE $n
    Like(String),
    /// column ILIKE $n (case-insensitive)
    ILike(String),
    /// column IN ($n, $n+1, ...)
    In(String, usize),
    /// column NOT IN ($n, $n+1, ...)
    NotIn(String, usize),
    /// column BETWEEN $n AND $n+1
    Between(String),
    /// column IS NULL
    IsNull(String),
    /// column IS NOT NULL
    IsNotNull(String),
    /// Custom SQL condition (use with caution, ensure no injection)
    Raw(String),
}

impl Condition {
    /// Get the number of parameters this condition requires
    pub fn param_count(&self) -> usize {
        match self {
            Condition::Eq(_)
            | Condition::NotEq(_)
            | Condition::Gt(_)
            | Condition::Gte(_)
            | Condition::Lt(_)
            | Condition::Lte(_)
            | Condition::Like(_)
            | Condition::ILike(_) => 1,
            Condition::In(_, count) | Condition::NotIn(_, count) => *count,
            Condition::Between(_) => 2,
            Condition::IsNull(_) | Condition::IsNotNull(_) | Condition::Raw(_) => 0,
        }
    }

    /// Build the SQL fragment for this condition
    pub fn to_sql(&self, param_start: usize) -> String {
        match self {
            Condition::Eq(col) => format!("{col} = ${param_start}"),
            Condition::NotEq(col) => format!("{col} != ${param_start}"),
            Condition::Gt(col) => format!("{col} > ${param_start}"),
            Condition::Gte(col) => format!("{col} >= ${param_start}"),
            Condition::Lt(col) => format!("{col} < ${param_start}"),
            Condition::Lte(col) => format!("{col} <= ${param_start}"),
            Condition::Like(col) => format!("{col} LIKE ${param_start}"),
            Condition::ILike(col) => format!("{col} ILIKE ${param_start}"),
            Condition::In(col, count) => {
                let placeholders: Vec<String> = (param_start..param_start + count)
                    .map(|i| format!("${i}"))
                    .collect();
                format!("{col} IN ({})", placeholders.join(", "))
            }
            Condition::NotIn(col, count) => {
                let placeholders: Vec<String> = (param_start..param_start + count)
                    .map(|i| format!("${i}"))
                    .collect();
                format!("{col} NOT IN ({})", placeholders.join(", "))
            }
            Condition::Between(col) => {
                format!("{col} BETWEEN ${param_start} AND ${}", param_start + 1)
            }
            Condition::IsNull(col) => format!("{col} IS NULL"),
            Condition::IsNotNull(col) => format!("{col} IS NOT NULL"),
            Condition::Raw(sql) => sql.clone(),
        }
    }
}

/// JOIN types for table joins
#[derive(Debug, Clone)]
pub enum JoinType {
    /// INNER JOIN
    Inner,
    /// LEFT JOIN
    Left,
    /// RIGHT JOIN
    Right,
    /// FULL OUTER JOIN
    Full,
}

impl fmt::Display for JoinType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JoinType::Inner => write!(f, "INNER JOIN"),
            JoinType::Left => write!(f, "LEFT JOIN"),
            JoinType::Right => write!(f, "RIGHT JOIN"),
            JoinType::Full => write!(f, "FULL OUTER JOIN"),
        }
    }
}

/// JOIN clause definition
#[derive(Debug, Clone)]
pub struct Join {
    join_type: JoinType,
    table: String,
    on_condition: String,
}

/// SQL query builder for SELECT statements
#[derive(Debug, Clone)]
pub struct QueryBuilder {
    table: String,
    select_columns: Vec<String>,
    conditions: Vec<Condition>,
    joins: Vec<Join>,
    order_by: Vec<(String, SortOrder)>,
    group_by: Vec<String>,
    having: Vec<Condition>,
    limit_value: Option<i64>,
    offset_value: Option<i64>,
    distinct: bool,
}

impl QueryBuilder {
    /// Create a new query builder for the specified table
    ///
    /// # Example
    ///
    /// ```rust
    /// use oxify_storage::query_builder::QueryBuilder;
    ///
    /// let builder = QueryBuilder::new("users");
    /// ```
    pub fn new(table: &str) -> Self {
        Self {
            table: table.to_string(),
            select_columns: vec!["*".to_string()],
            conditions: Vec::new(),
            joins: Vec::new(),
            order_by: Vec::new(),
            group_by: Vec::new(),
            having: Vec::new(),
            limit_value: None,
            offset_value: None,
            distinct: false,
        }
    }

    /// Specify columns to select
    ///
    /// # Example
    ///
    /// ```rust
    /// use oxify_storage::query_builder::QueryBuilder;
    ///
    /// let builder = QueryBuilder::new("users")
    ///     .select(&["id", "name", "email"]);
    /// ```
    pub fn select(mut self, columns: &[&str]) -> Self {
        self.select_columns = columns.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Add SELECT DISTINCT
    pub fn distinct(mut self) -> Self {
        self.distinct = true;
        self
    }

    /// Add a WHERE condition
    ///
    /// # Example
    ///
    /// ```rust
    /// use oxify_storage::query_builder::{QueryBuilder, Condition};
    ///
    /// let builder = QueryBuilder::new("users")
    ///     .where_condition(Condition::Eq("status".to_string()));
    /// ```
    pub fn where_condition(mut self, condition: Condition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Add a WHERE condition only if the predicate is true
    ///
    /// This is useful for optional filters
    pub fn where_if(self, predicate: bool, condition: Condition) -> Self {
        if predicate {
            self.where_condition(condition)
        } else {
            self
        }
    }

    /// Add a JOIN clause
    pub fn join(mut self, join_type: JoinType, table: &str, on_condition: &str) -> Self {
        self.joins.push(Join {
            join_type,
            table: table.to_string(),
            on_condition: on_condition.to_string(),
        });
        self
    }

    /// Add an ORDER BY clause
    ///
    /// # Example
    ///
    /// ```rust
    /// use oxify_storage::query_builder::{QueryBuilder, SortOrder};
    ///
    /// let builder = QueryBuilder::new("users")
    ///     .order_by("created_at", SortOrder::Desc);
    /// ```
    pub fn order_by(mut self, column: &str, order: SortOrder) -> Self {
        self.order_by.push((column.to_string(), order));
        self
    }

    /// Add a GROUP BY clause
    pub fn group_by(mut self, column: &str) -> Self {
        self.group_by.push(column.to_string());
        self
    }

    /// Add a HAVING condition (requires GROUP BY)
    pub fn having(mut self, condition: Condition) -> Self {
        self.having.push(condition);
        self
    }

    /// Set LIMIT
    ///
    /// # Example
    ///
    /// ```rust
    /// use oxify_storage::query_builder::QueryBuilder;
    ///
    /// let builder = QueryBuilder::new("users").limit(10);
    /// ```
    pub fn limit(mut self, limit: i64) -> Self {
        self.limit_value = Some(limit);
        self
    }

    /// Set OFFSET
    pub fn offset(mut self, offset: i64) -> Self {
        self.offset_value = Some(offset);
        self
    }

    /// Build the final SQL query and return the query string and total parameter count
    ///
    /// Returns a tuple of (SQL string, number of parameters expected)
    ///
    /// # Example
    ///
    /// ```rust
    /// use oxify_storage::query_builder::{QueryBuilder, Condition};
    ///
    /// let mut builder = QueryBuilder::new("users")
    ///     .select(&["id", "name"])
    ///     .where_condition(Condition::Eq("status".to_string()));
    ///
    /// let (sql, param_count) = builder.build();
    /// assert_eq!(param_count, 1);
    /// ```
    pub fn build(&mut self) -> (String, usize) {
        let mut sql = String::new();
        let mut param_counter = 1;

        // SELECT clause
        sql.push_str("SELECT ");
        if self.distinct {
            sql.push_str("DISTINCT ");
        }
        sql.push_str(&self.select_columns.join(", "));

        // FROM clause
        sql.push_str(&format!(" FROM {}", self.table));

        // JOIN clauses
        for join in &self.joins {
            sql.push_str(&format!(
                " {} {} ON {}",
                join.join_type, join.table, join.on_condition
            ));
        }

        // WHERE clause
        if !self.conditions.is_empty() {
            sql.push_str(" WHERE ");
            let where_clauses: Vec<String> = self
                .conditions
                .iter()
                .map(|cond| {
                    let clause = cond.to_sql(param_counter);
                    param_counter += cond.param_count();
                    clause
                })
                .collect();
            sql.push_str(&where_clauses.join(" AND "));
        }

        // GROUP BY clause
        if !self.group_by.is_empty() {
            sql.push_str(" GROUP BY ");
            sql.push_str(&self.group_by.join(", "));
        }

        // HAVING clause
        if !self.having.is_empty() {
            sql.push_str(" HAVING ");
            let having_clauses: Vec<String> = self
                .having
                .iter()
                .map(|cond| {
                    let clause = cond.to_sql(param_counter);
                    param_counter += cond.param_count();
                    clause
                })
                .collect();
            sql.push_str(&having_clauses.join(" AND "));
        }

        // ORDER BY clause
        if !self.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            let order_clauses: Vec<String> = self
                .order_by
                .iter()
                .map(|(col, order)| format!("{col} {order}"))
                .collect();
            sql.push_str(&order_clauses.join(", "));
        }

        // LIMIT clause
        if let Some(limit) = self.limit_value {
            sql.push_str(&format!(" LIMIT {limit}"));
        }

        // OFFSET clause
        if let Some(offset) = self.offset_value {
            sql.push_str(&format!(" OFFSET {offset}"));
        }

        (sql, param_counter - 1)
    }

    /// Build a COUNT query from this builder
    ///
    /// This creates a COUNT(*) query using the same WHERE conditions
    pub fn build_count(&mut self) -> (String, usize) {
        let original_select = self.select_columns.clone();
        let original_order = self.order_by.clone();
        let original_limit = self.limit_value;
        let original_offset = self.offset_value;

        self.select_columns = vec!["COUNT(*)".to_string()];
        self.order_by.clear();
        self.limit_value = None;
        self.offset_value = None;

        let result = self.build();

        self.select_columns = original_select;
        self.order_by = original_order;
        self.limit_value = original_limit;
        self.offset_value = original_offset;

        result
    }
}

/// Builder for UPDATE queries
#[derive(Debug, Clone)]
pub struct UpdateBuilder {
    table: String,
    set_columns: Vec<String>,
    conditions: Vec<Condition>,
}

impl UpdateBuilder {
    /// Create a new UPDATE query builder
    pub fn new(table: &str) -> Self {
        Self {
            table: table.to_string(),
            set_columns: Vec::new(),
            conditions: Vec::new(),
        }
    }

    /// Add a column to SET
    pub fn set(mut self, column: &str) -> Self {
        self.set_columns.push(column.to_string());
        self
    }

    /// Add a WHERE condition
    pub fn where_condition(mut self, condition: Condition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Build the UPDATE query
    pub fn build(&self) -> (String, usize) {
        let mut sql = format!("UPDATE {}", self.table);
        let mut param_counter = 1;

        // SET clause
        if !self.set_columns.is_empty() {
            sql.push_str(" SET ");
            let set_clauses: Vec<String> = self
                .set_columns
                .iter()
                .map(|col| {
                    let clause = format!("{col} = ${param_counter}");
                    param_counter += 1;
                    clause
                })
                .collect();
            sql.push_str(&set_clauses.join(", "));
        }

        // WHERE clause
        if !self.conditions.is_empty() {
            sql.push_str(" WHERE ");
            let where_clauses: Vec<String> = self
                .conditions
                .iter()
                .map(|cond| {
                    let clause = cond.to_sql(param_counter);
                    param_counter += cond.param_count();
                    clause
                })
                .collect();
            sql.push_str(&where_clauses.join(" AND "));
        }

        (sql, param_counter - 1)
    }
}

/// Builder for DELETE queries
#[derive(Debug, Clone)]
pub struct DeleteBuilder {
    table: String,
    conditions: Vec<Condition>,
}

impl DeleteBuilder {
    /// Create a new DELETE query builder
    pub fn new(table: &str) -> Self {
        Self {
            table: table.to_string(),
            conditions: Vec::new(),
        }
    }

    /// Add a WHERE condition
    pub fn where_condition(mut self, condition: Condition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Build the DELETE query
    pub fn build(&self) -> (String, usize) {
        let mut sql = format!("DELETE FROM {}", self.table);
        let mut param_counter = 1;

        // WHERE clause
        if !self.conditions.is_empty() {
            sql.push_str(" WHERE ");
            let where_clauses: Vec<String> = self
                .conditions
                .iter()
                .map(|cond| {
                    let clause = cond.to_sql(param_counter);
                    param_counter += cond.param_count();
                    clause
                })
                .collect();
            sql.push_str(&where_clauses.join(" AND "));
        }

        (sql, param_counter - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_select() {
        let mut builder = QueryBuilder::new("users");
        let (sql, params) = builder.build();
        assert_eq!(sql, "SELECT * FROM users");
        assert_eq!(params, 0);
    }

    #[test]
    fn test_select_with_columns() {
        let mut builder = QueryBuilder::new("users").select(&["id", "name", "email"]);
        let (sql, params) = builder.build();
        assert_eq!(sql, "SELECT id, name, email FROM users");
        assert_eq!(params, 0);
    }

    #[test]
    fn test_where_equal() {
        let mut builder =
            QueryBuilder::new("users").where_condition(Condition::Eq("status".to_string()));
        let (sql, params) = builder.build();
        assert_eq!(sql, "SELECT * FROM users WHERE status = $1");
        assert_eq!(params, 1);
    }

    #[test]
    fn test_multiple_where_conditions() {
        let mut builder = QueryBuilder::new("users")
            .where_condition(Condition::Eq("status".to_string()))
            .where_condition(Condition::Gt("age".to_string()));
        let (sql, params) = builder.build();
        assert_eq!(sql, "SELECT * FROM users WHERE status = $1 AND age > $2");
        assert_eq!(params, 2);
    }

    #[test]
    fn test_where_in() {
        let mut builder =
            QueryBuilder::new("users").where_condition(Condition::In("role".to_string(), 3));
        let (sql, params) = builder.build();
        assert_eq!(sql, "SELECT * FROM users WHERE role IN ($1, $2, $3)");
        assert_eq!(params, 3);
    }

    #[test]
    fn test_where_between() {
        let mut builder =
            QueryBuilder::new("users").where_condition(Condition::Between("age".to_string()));
        let (sql, params) = builder.build();
        assert_eq!(sql, "SELECT * FROM users WHERE age BETWEEN $1 AND $2");
        assert_eq!(params, 2);
    }

    #[test]
    fn test_where_null() {
        let mut builder =
            QueryBuilder::new("users").where_condition(Condition::IsNull("deleted_at".to_string()));
        let (sql, params) = builder.build();
        assert_eq!(sql, "SELECT * FROM users WHERE deleted_at IS NULL");
        assert_eq!(params, 0);
    }

    #[test]
    fn test_order_by() {
        let mut builder = QueryBuilder::new("users")
            .order_by("created_at", SortOrder::Desc)
            .order_by("name", SortOrder::Asc);
        let (sql, params) = builder.build();
        assert_eq!(
            sql,
            "SELECT * FROM users ORDER BY created_at DESC, name ASC"
        );
        assert_eq!(params, 0);
    }

    #[test]
    fn test_limit_offset() {
        let mut builder = QueryBuilder::new("users").limit(10).offset(20);
        let (sql, params) = builder.build();
        assert_eq!(sql, "SELECT * FROM users LIMIT 10 OFFSET 20");
        assert_eq!(params, 0);
    }

    #[test]
    fn test_complex_query() {
        let mut builder = QueryBuilder::new("users")
            .select(&["id", "name"])
            .where_condition(Condition::Eq("status".to_string()))
            .where_condition(Condition::IsNotNull("email".to_string()))
            .order_by("created_at", SortOrder::Desc)
            .limit(20);
        let (sql, params) = builder.build();
        assert_eq!(
            sql,
            "SELECT id, name FROM users WHERE status = $1 AND email IS NOT NULL ORDER BY created_at DESC LIMIT 20"
        );
        assert_eq!(params, 1);
    }

    #[test]
    fn test_count_query() {
        let mut builder = QueryBuilder::new("users")
            .select(&["id", "name"])
            .where_condition(Condition::Eq("status".to_string()))
            .order_by("created_at", SortOrder::Desc)
            .limit(20);
        let (sql, params) = builder.build_count();
        assert_eq!(sql, "SELECT COUNT(*) FROM users WHERE status = $1");
        assert_eq!(params, 1);
    }

    #[test]
    fn test_distinct_query() {
        let mut builder = QueryBuilder::new("users").select(&["email"]).distinct();
        let (sql, params) = builder.build();
        assert_eq!(sql, "SELECT DISTINCT email FROM users");
        assert_eq!(params, 0);
    }

    #[test]
    fn test_group_by() {
        let mut builder = QueryBuilder::new("users")
            .select(&["role", "COUNT(*) as count"])
            .group_by("role");
        let (sql, params) = builder.build();
        assert_eq!(
            sql,
            "SELECT role, COUNT(*) as count FROM users GROUP BY role"
        );
        assert_eq!(params, 0);
    }

    #[test]
    fn test_update_builder() {
        let builder = UpdateBuilder::new("users")
            .set("name")
            .set("email")
            .where_condition(Condition::Eq("id".to_string()));
        let (sql, params) = builder.build();
        assert_eq!(sql, "UPDATE users SET name = $1, email = $2 WHERE id = $3");
        assert_eq!(params, 3);
    }

    #[test]
    fn test_delete_builder() {
        let builder =
            DeleteBuilder::new("users").where_condition(Condition::Lt("last_login".to_string()));
        let (sql, params) = builder.build();
        assert_eq!(sql, "DELETE FROM users WHERE last_login < $1");
        assert_eq!(params, 1);
    }

    #[test]
    fn test_join() {
        let mut builder = QueryBuilder::new("users")
            .join(JoinType::Inner, "roles", "users.role_id = roles.id")
            .select(&["users.name", "roles.role_name"]);
        let (sql, params) = builder.build();
        assert_eq!(
            sql,
            "SELECT users.name, roles.role_name FROM users INNER JOIN roles ON users.role_id = roles.id"
        );
        assert_eq!(params, 0);
    }

    #[test]
    fn test_where_if() {
        let mut builder1 =
            QueryBuilder::new("users").where_if(true, Condition::Eq("status".to_string()));
        let (sql1, params1) = builder1.build();
        assert_eq!(sql1, "SELECT * FROM users WHERE status = $1");
        assert_eq!(params1, 1);

        let mut builder2 =
            QueryBuilder::new("users").where_if(false, Condition::Eq("status".to_string()));
        let (sql2, params2) = builder2.build();
        assert_eq!(sql2, "SELECT * FROM users");
        assert_eq!(params2, 0);
    }

    #[test]
    fn test_like_condition() {
        let mut builder =
            QueryBuilder::new("users").where_condition(Condition::Like("name".to_string()));
        let (sql, params) = builder.build();
        assert_eq!(sql, "SELECT * FROM users WHERE name LIKE $1");
        assert_eq!(params, 1);
    }

    #[test]
    fn test_having_clause() {
        let mut builder = QueryBuilder::new("orders")
            .select(&["customer_id", "COUNT(*) as order_count"])
            .group_by("customer_id")
            .having(Condition::Gt("COUNT(*)".to_string()));
        let (sql, params) = builder.build();
        assert_eq!(
            sql,
            "SELECT customer_id, COUNT(*) as order_count FROM orders GROUP BY customer_id HAVING COUNT(*) > $1"
        );
        assert_eq!(params, 1);
    }
}
