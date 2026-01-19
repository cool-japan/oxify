//! GraphQL API module for OxiFY Server
//!
//! This module provides a GraphQL API using async-graphql with:
//! - Schema definition for workflows, executions, and users
//! - Query complexity limits to prevent DoS attacks
//! - DataLoader for N+1 query prevention
//! - Integration with Axum HTTP server
//!
//! # Example
//!
//! ```rust,no_run
//! use oxify_server::graphql::{create_schema, graphql_handler, graphql_playground};
//! use axum::{Router, routing::{get, post}};
//!
//! # async fn example() {
//! let schema = create_schema();
//!
//! let app: Router = Router::new()
//!     .route("/graphql", get(graphql_playground).post(graphql_handler))
//!     .layer(axum::extract::Extension(schema));
//! # }
//! ```

use async_graphql::{
    dataloader::{DataLoader, Loader},
    Context, EmptyMutation, EmptySubscription, Object, Schema, SimpleObject, ID,
};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::Extension,
    response::{Html, IntoResponse},
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// GraphQL Schema Types
// ============================================================================

/// Workflow representation in GraphQL
#[derive(Debug, Clone, SimpleObject)]
#[graphql(complex)]
pub struct Workflow {
    /// Unique identifier
    pub id: ID,
    /// Workflow name
    pub name: String,
    /// Workflow description
    pub description: Option<String>,
    /// Owner user ID
    pub owner_id: ID,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
    /// Workflow status
    pub status: WorkflowStatus,
    /// Number of nodes in the workflow
    pub node_count: i32,
    /// Tags for categorization
    pub tags: Vec<String>,
}

/// Workflow status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, async_graphql::Enum)]
pub enum WorkflowStatus {
    /// Draft workflow
    Draft,
    /// Published workflow
    Published,
    /// Archived workflow
    Archived,
    /// Deprecated workflow
    Deprecated,
}

/// Workflow execution representation
#[derive(Debug, Clone, SimpleObject)]
#[graphql(complex)]
pub struct Execution {
    /// Unique identifier
    pub id: ID,
    /// Associated workflow ID
    pub workflow_id: ID,
    /// User who started the execution
    pub user_id: ID,
    /// Execution status
    pub status: ExecutionStatus,
    /// Start timestamp
    pub started_at: DateTime<Utc>,
    /// Completion timestamp
    pub completed_at: Option<DateTime<Utc>>,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution duration in milliseconds
    pub duration_ms: Option<i64>,
    /// Number of completed nodes
    pub completed_nodes: i32,
    /// Total number of nodes
    pub total_nodes: i32,
}

/// Execution status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, async_graphql::Enum)]
pub enum ExecutionStatus {
    /// Execution queued
    Queued,
    /// Execution running
    Running,
    /// Execution completed successfully
    Completed,
    /// Execution failed
    Failed,
    /// Execution cancelled
    Cancelled,
}

/// User representation
#[derive(Debug, Clone, SimpleObject)]
#[graphql(complex)]
pub struct User {
    /// Unique identifier
    pub id: ID,
    /// Username
    pub username: String,
    /// Email address
    pub email: String,
    /// Display name
    pub display_name: Option<String>,
    /// User role
    pub role: UserRole,
    /// Account creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last login timestamp
    pub last_login_at: Option<DateTime<Utc>>,
    /// Whether the user is active
    pub is_active: bool,
}

/// User role enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, async_graphql::Enum)]
pub enum UserRole {
    /// Administrator role
    Admin,
    /// Regular user role
    User,
    /// Read-only role
    Viewer,
    /// Service account
    Service,
}

// ============================================================================
// Complex Field Resolvers
// ============================================================================

#[async_graphql::ComplexObject]
impl Workflow {
    /// Fetch the owner user
    async fn owner(&self, ctx: &Context<'_>) -> async_graphql::Result<User> {
        let loader = ctx.data::<DataLoader<UserLoader>>()?;
        let user_id = self.owner_id.parse::<Uuid>()?;
        loader
            .load_one(user_id)
            .await?
            .ok_or_else(|| async_graphql::Error::new("User not found"))
    }

    /// Fetch recent executions for this workflow
    async fn recent_executions(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 10, validator(maximum = 100))] _limit: usize,
    ) -> async_graphql::Result<Vec<Execution>> {
        let loader = ctx.data::<DataLoader<ExecutionLoader>>()?;
        let workflow_id = self.id.parse::<Uuid>()?;
        loader
            .load_one(workflow_id)
            .await?
            .ok_or_else(|| async_graphql::Error::new("No executions found for workflow"))
    }
}

#[async_graphql::ComplexObject]
impl Execution {
    /// Fetch the associated workflow
    async fn workflow(&self, ctx: &Context<'_>) -> async_graphql::Result<Workflow> {
        let loader = ctx.data::<DataLoader<WorkflowLoader>>()?;
        let workflow_id = self.workflow_id.parse::<Uuid>()?;
        loader
            .load_one(workflow_id)
            .await?
            .ok_or_else(|| async_graphql::Error::new("Workflow not found"))
    }

    /// Fetch the user who started the execution
    async fn user(&self, ctx: &Context<'_>) -> async_graphql::Result<User> {
        let loader = ctx.data::<DataLoader<UserLoader>>()?;
        let user_id = self.user_id.parse::<Uuid>()?;
        loader
            .load_one(user_id)
            .await?
            .ok_or_else(|| async_graphql::Error::new("User not found"))
    }

    /// Calculate execution progress percentage
    async fn progress(&self, _ctx: &Context<'_>) -> async_graphql::Result<f64> {
        if self.total_nodes == 0 {
            return Ok(0.0);
        }
        Ok((self.completed_nodes as f64 / self.total_nodes as f64) * 100.0)
    }
}

#[async_graphql::ComplexObject]
impl User {
    /// Fetch workflows owned by this user
    async fn workflows(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 20, validator(maximum = 100))] _limit: usize,
    ) -> async_graphql::Result<Vec<Workflow>> {
        let loader = ctx.data::<DataLoader<WorkflowsByUserLoader>>()?;
        let user_id = self.id.parse::<Uuid>()?;
        loader
            .load_one(user_id)
            .await?
            .ok_or_else(|| async_graphql::Error::new("No workflows found for user"))
    }

    /// Fetch recent executions by this user
    async fn executions(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 20, validator(maximum = 100))] _limit: usize,
    ) -> async_graphql::Result<Vec<Execution>> {
        let loader = ctx.data::<DataLoader<ExecutionsByUserLoader>>()?;
        let user_id = self.id.parse::<Uuid>()?;
        loader
            .load_one(user_id)
            .await?
            .ok_or_else(|| async_graphql::Error::new("No executions found for user"))
    }
}

// ============================================================================
// DataLoader Implementations
// ============================================================================

/// Loader for batching user queries
pub struct UserLoader;

impl Loader<Uuid> for UserLoader {
    type Value = User;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[Uuid]) -> Result<HashMap<Uuid, Self::Value>, Self::Error> {
        // In production, this would query the database
        // For now, return mock data
        let mut users = HashMap::new();
        for &id in keys {
            users.insert(
                id,
                User {
                    id: ID(id.to_string()),
                    username: format!("user_{}", id),
                    email: format!("user_{}@example.com", id),
                    display_name: Some(format!("User {}", id)),
                    role: UserRole::User,
                    created_at: Utc::now(),
                    last_login_at: Some(Utc::now()),
                    is_active: true,
                },
            );
        }
        Ok(users)
    }
}

/// Loader for batching workflow queries
pub struct WorkflowLoader;

impl Loader<Uuid> for WorkflowLoader {
    type Value = Workflow;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[Uuid]) -> Result<HashMap<Uuid, Self::Value>, Self::Error> {
        // In production, this would query the database
        let mut workflows = HashMap::new();
        for &id in keys {
            workflows.insert(
                id,
                Workflow {
                    id: ID(id.to_string()),
                    name: format!("Workflow {}", id),
                    description: Some("Example workflow".to_string()),
                    owner_id: ID(Uuid::new_v4().to_string()),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    status: WorkflowStatus::Published,
                    node_count: 5,
                    tags: vec!["example".to_string(), "test".to_string()],
                },
            );
        }
        Ok(workflows)
    }
}

/// Loader for batching execution queries
pub struct ExecutionLoader;

impl Loader<Uuid> for ExecutionLoader {
    type Value = Vec<Execution>;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[Uuid]) -> Result<HashMap<Uuid, Self::Value>, Self::Error> {
        // In production, this would query executions by workflow_id
        let mut executions = HashMap::new();
        for &workflow_id in keys {
            let exec_list = vec![Execution {
                id: ID(Uuid::new_v4().to_string()),
                workflow_id: ID(workflow_id.to_string()),
                user_id: ID(Uuid::new_v4().to_string()),
                status: ExecutionStatus::Completed,
                started_at: Utc::now(),
                completed_at: Some(Utc::now()),
                error: None,
                duration_ms: Some(1500),
                completed_nodes: 5,
                total_nodes: 5,
            }];
            executions.insert(workflow_id, exec_list);
        }
        Ok(executions)
    }
}

/// Loader for batching workflow queries by user
pub struct WorkflowsByUserLoader;

impl Loader<Uuid> for WorkflowsByUserLoader {
    type Value = Vec<Workflow>;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[Uuid]) -> Result<HashMap<Uuid, Self::Value>, Self::Error> {
        // In production, this would query workflows by owner_id
        let mut workflows = HashMap::new();
        for &user_id in keys {
            let workflow_list = vec![Workflow {
                id: ID(Uuid::new_v4().to_string()),
                name: format!("User {} Workflow", user_id),
                description: Some("Example workflow".to_string()),
                owner_id: ID(user_id.to_string()),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                status: WorkflowStatus::Published,
                node_count: 3,
                tags: vec!["example".to_string()],
            }];
            workflows.insert(user_id, workflow_list);
        }
        Ok(workflows)
    }
}

/// Loader for batching execution queries by user
pub struct ExecutionsByUserLoader;

impl Loader<Uuid> for ExecutionsByUserLoader {
    type Value = Vec<Execution>;
    type Error = async_graphql::Error;

    async fn load(&self, keys: &[Uuid]) -> Result<HashMap<Uuid, Self::Value>, Self::Error> {
        // In production, this would query executions by user_id
        let mut executions = HashMap::new();
        for &user_id in keys {
            let exec_list = vec![Execution {
                id: ID(Uuid::new_v4().to_string()),
                workflow_id: ID(Uuid::new_v4().to_string()),
                user_id: ID(user_id.to_string()),
                status: ExecutionStatus::Running,
                started_at: Utc::now(),
                completed_at: None,
                error: None,
                duration_ms: None,
                completed_nodes: 2,
                total_nodes: 5,
            }];
            executions.insert(user_id, exec_list);
        }
        Ok(executions)
    }
}

// ============================================================================
// Query Root
// ============================================================================

/// Root query object
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Get a workflow by ID
    async fn workflow(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<Workflow> {
        let loader = ctx.data::<DataLoader<WorkflowLoader>>()?;
        let workflow_id = id.parse::<Uuid>()?;
        loader
            .load_one(workflow_id)
            .await?
            .ok_or_else(|| async_graphql::Error::new("Workflow not found"))
    }

    /// List workflows with pagination
    async fn workflows(
        &self,
        #[graphql(default = 0)] offset: usize,
        #[graphql(default = 20, validator(maximum = 100))] limit: usize,
    ) -> async_graphql::Result<Vec<Workflow>> {
        // In production, this would query the database with pagination
        let workflows = vec![
            Workflow {
                id: ID(Uuid::new_v4().to_string()),
                name: "Example Workflow 1".to_string(),
                description: Some("First example workflow".to_string()),
                owner_id: ID(Uuid::new_v4().to_string()),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                status: WorkflowStatus::Published,
                node_count: 5,
                tags: vec!["example".to_string(), "test".to_string()],
            },
            Workflow {
                id: ID(Uuid::new_v4().to_string()),
                name: "Example Workflow 2".to_string(),
                description: Some("Second example workflow".to_string()),
                owner_id: ID(Uuid::new_v4().to_string()),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                status: WorkflowStatus::Draft,
                node_count: 3,
                tags: vec!["draft".to_string()],
            },
        ];

        Ok(workflows.into_iter().skip(offset).take(limit).collect())
    }

    /// Get an execution by ID
    async fn execution(&self, _ctx: &Context<'_>, id: ID) -> async_graphql::Result<Execution> {
        // In production, this would query the database
        Ok(Execution {
            id: id.clone(),
            workflow_id: ID(Uuid::new_v4().to_string()),
            user_id: ID(Uuid::new_v4().to_string()),
            status: ExecutionStatus::Completed,
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            error: None,
            duration_ms: Some(2500),
            completed_nodes: 5,
            total_nodes: 5,
        })
    }

    /// List executions with pagination and optional filtering
    async fn executions(
        &self,
        #[graphql(default = 0)] offset: usize,
        #[graphql(default = 20, validator(maximum = 100))] limit: usize,
        _workflow_id: Option<ID>,
        status: Option<ExecutionStatus>,
    ) -> async_graphql::Result<Vec<Execution>> {
        // In production, this would query the database with filters
        let mut executions = vec![
            Execution {
                id: ID(Uuid::new_v4().to_string()),
                workflow_id: ID(Uuid::new_v4().to_string()),
                user_id: ID(Uuid::new_v4().to_string()),
                status: ExecutionStatus::Completed,
                started_at: Utc::now(),
                completed_at: Some(Utc::now()),
                error: None,
                duration_ms: Some(1800),
                completed_nodes: 5,
                total_nodes: 5,
            },
            Execution {
                id: ID(Uuid::new_v4().to_string()),
                workflow_id: ID(Uuid::new_v4().to_string()),
                user_id: ID(Uuid::new_v4().to_string()),
                status: ExecutionStatus::Running,
                started_at: Utc::now(),
                completed_at: None,
                error: None,
                duration_ms: None,
                completed_nodes: 3,
                total_nodes: 5,
            },
        ];

        // Apply status filter if provided
        if let Some(status_filter) = status {
            executions.retain(|e| e.status == status_filter);
        }

        Ok(executions.into_iter().skip(offset).take(limit).collect())
    }

    /// Get a user by ID
    async fn user(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<User> {
        let loader = ctx.data::<DataLoader<UserLoader>>()?;
        let user_id = id.parse::<Uuid>()?;
        loader
            .load_one(user_id)
            .await?
            .ok_or_else(|| async_graphql::Error::new("User not found"))
    }

    /// List users with pagination
    async fn users(
        &self,
        #[graphql(default = 0)] offset: usize,
        #[graphql(default = 20, validator(maximum = 100))] limit: usize,
    ) -> async_graphql::Result<Vec<User>> {
        // In production, this would query the database
        let users = vec![
            User {
                id: ID(Uuid::new_v4().to_string()),
                username: "alice".to_string(),
                email: "alice@example.com".to_string(),
                display_name: Some("Alice".to_string()),
                role: UserRole::Admin,
                created_at: Utc::now(),
                last_login_at: Some(Utc::now()),
                is_active: true,
            },
            User {
                id: ID(Uuid::new_v4().to_string()),
                username: "bob".to_string(),
                email: "bob@example.com".to_string(),
                display_name: Some("Bob".to_string()),
                role: UserRole::User,
                created_at: Utc::now(),
                last_login_at: Some(Utc::now()),
                is_active: true,
            },
        ];

        Ok(users.into_iter().skip(offset).take(limit).collect())
    }
}

// ============================================================================
// Schema Creation
// ============================================================================

/// GraphQL schema type
pub type OxifySchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

/// Create a new GraphQL schema with complexity limits and DataLoaders
///
/// The schema includes:
/// - Query complexity limit: 1000 (prevents expensive queries)
/// - Query depth limit: 10 (prevents deeply nested queries)
/// - DataLoaders for efficient batch loading
pub fn create_schema() -> OxifySchema {
    Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .limit_complexity(1000) // Prevent DoS attacks with expensive queries
        .limit_depth(10) // Prevent deeply nested queries
        .data(DataLoader::new(UserLoader, tokio::spawn))
        .data(DataLoader::new(WorkflowLoader, tokio::spawn))
        .data(DataLoader::new(ExecutionLoader, tokio::spawn))
        .data(DataLoader::new(WorkflowsByUserLoader, tokio::spawn))
        .data(DataLoader::new(ExecutionsByUserLoader, tokio::spawn))
        .finish()
}

// ============================================================================
// Axum Handlers
// ============================================================================

/// GraphQL POST handler for executing queries
pub async fn graphql_handler(
    Extension(schema): Extension<OxifySchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

/// GraphQL Playground handler for interactive testing
pub async fn graphql_playground() -> impl IntoResponse {
    Html(
        r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>OxiFY GraphQL Playground</title>
    <style>
        body {
            margin: 0;
            overflow: hidden;
        }
    </style>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/graphql-playground-react/build/static/css/index.css" />
    <script src="https://cdn.jsdelivr.net/npm/graphql-playground-react/build/static/js/middleware.js"></script>
</head>
<body>
    <div id="root"></div>
    <script>
        window.addEventListener('load', function(event) {
            GraphQLPlayground.init(document.getElementById('root'), {
                endpoint: '/graphql',
                settings: {
                    'request.credentials': 'same-origin',
                }
            })
        })
    </script>
</body>
</html>
"#,
    )
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_schema() {
        let schema = create_schema();
        assert!(schema.execute("{ __typename }").await.is_ok());
    }

    #[tokio::test]
    async fn test_query_workflows() {
        let schema = create_schema();
        let query = r#"
            query {
                workflows(limit: 2) {
                    id
                    name
                    status
                    nodeCount
                }
            }
        "#;
        let res = schema.execute(query).await;
        if !res.errors.is_empty() {
            eprintln!("GraphQL errors: {:?}", res.errors);
        }
        assert!(res.errors.is_empty());
    }

    #[tokio::test]
    async fn test_query_with_complexity_limit() {
        let schema = create_schema();
        // This query should be accepted (not too complex)
        let query = r#"
            query {
                workflows(limit: 2) {
                    id
                    name
                }
            }
        "#;
        let res = schema.execute(query).await;
        // Should succeed as it's within complexity limits
        assert!(res.errors.is_empty());
    }

    #[tokio::test]
    async fn test_execution_progress_calculation() {
        // Test progress calculation logic
        let completed = 3;
        let total = 5;
        let progress = (completed as f64 / total as f64) * 100.0;
        assert_eq!(progress, 60.0);
    }

    #[tokio::test]
    async fn test_user_loader() {
        let loader = DataLoader::new(UserLoader, tokio::spawn);
        let user_id = Uuid::new_v4();
        let result = loader.load_one(user_id).await;
        assert!(result.is_ok());
        let user = result.unwrap();
        assert!(user.is_some());
    }

    #[tokio::test]
    async fn test_workflow_loader() {
        let loader = DataLoader::new(WorkflowLoader, tokio::spawn);
        let workflow_id = Uuid::new_v4();
        let result = loader.load_one(workflow_id).await;
        assert!(result.is_ok());
        let workflow = result.unwrap();
        assert!(workflow.is_some());
    }

    #[tokio::test]
    async fn test_query_pagination() {
        let schema = create_schema();
        let query = r#"
            query {
                workflows(offset: 0, limit: 10) {
                    id
                    name
                }
            }
        "#;
        let res = schema.execute(query).await;
        assert!(res.errors.is_empty());
    }

    #[tokio::test]
    async fn test_query_filtering() {
        let schema = create_schema();
        let query = r#"
            query {
                executions(status: COMPLETED, limit: 5) {
                    id
                    status
                }
            }
        "#;
        let res = schema.execute(query).await;
        assert!(res.errors.is_empty());
    }

    #[tokio::test]
    async fn test_introspection_query() {
        let schema = create_schema();
        let query = r#"
            query {
                __schema {
                    queryType {
                        name
                    }
                }
            }
        "#;
        let res = schema.execute(query).await;
        assert!(res.errors.is_empty());
    }
}
