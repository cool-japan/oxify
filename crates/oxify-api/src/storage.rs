//! In-memory and database storage for workflows, executions, and users

use crate::user_types::ApiUser;
use oxify_model::{ExecutionContext, Workflow, WorkflowId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Storage for workflows
#[derive(Clone)]
pub struct WorkflowStore {
    workflows: Arc<RwLock<HashMap<WorkflowId, Workflow>>>,
}

impl WorkflowStore {
    pub fn new() -> Self {
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create(&self, workflow: Workflow) -> WorkflowId {
        let id = workflow.metadata.id;
        self.workflows.write().await.insert(id, workflow);
        id
    }

    pub async fn get(&self, id: &WorkflowId) -> Option<Workflow> {
        self.workflows.read().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<Workflow> {
        self.workflows.read().await.values().cloned().collect()
    }

    pub async fn update(&self, id: &WorkflowId, workflow: Workflow) -> Option<()> {
        let mut workflows = self.workflows.write().await;
        if workflows.contains_key(id) {
            workflows.insert(*id, workflow);
            Some(())
        } else {
            None
        }
    }

    pub async fn delete(&self, id: &WorkflowId) -> Option<Workflow> {
        self.workflows.write().await.remove(id)
    }
}

impl Default for WorkflowStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Storage for workflow executions
#[derive(Clone)]
pub struct ExecutionStore {
    executions: Arc<RwLock<HashMap<Uuid, ExecutionContext>>>,
}

impl ExecutionStore {
    pub fn new() -> Self {
        Self {
            executions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create(&self, execution: ExecutionContext) -> Uuid {
        let id = execution.execution_id;
        self.executions.write().await.insert(id, execution);
        id
    }

    pub async fn get(&self, id: &Uuid) -> Option<ExecutionContext> {
        self.executions.read().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<(Uuid, ExecutionContext)> {
        self.executions
            .read()
            .await
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect()
    }

    pub async fn list_by_workflow(
        &self,
        workflow_id: &WorkflowId,
    ) -> Vec<(Uuid, ExecutionContext)> {
        self.executions
            .read()
            .await
            .iter()
            .filter(|(_, ctx)| &ctx.workflow_id == workflow_id)
            .map(|(k, v)| (*k, v.clone()))
            .collect()
    }

    pub async fn update(&self, id: &Uuid, execution: ExecutionContext) -> Option<()> {
        let mut executions = self.executions.write().await;
        if executions.contains_key(id) {
            executions.insert(*id, execution);
            Some(())
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub async fn delete(&self, id: &Uuid) -> Option<ExecutionContext> {
        self.executions.write().await.remove(id)
    }
}

impl Default for ExecutionStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Storage for users
#[derive(Clone)]
pub struct UserStore {
    users: Arc<RwLock<HashMap<Uuid, ApiUser>>>,
    email_index: Arc<RwLock<HashMap<String, Uuid>>>,
}

impl UserStore {
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            email_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create(&self, user: ApiUser) {
        let id = user.id;
        let email = user.email.clone();
        self.users.write().await.insert(id, user);
        self.email_index.write().await.insert(email, id);
    }

    pub async fn get(&self, id: &Uuid) -> Option<ApiUser> {
        self.users.read().await.get(id).cloned()
    }

    pub async fn get_by_email(&self, email: &str) -> Option<ApiUser> {
        let email_index = self.email_index.read().await;
        let id = email_index.get(email)?;
        self.users.read().await.get(id).cloned()
    }

    #[allow(dead_code)]
    pub async fn list(&self) -> Vec<ApiUser> {
        self.users.read().await.values().cloned().collect()
    }

    #[allow(dead_code)]
    pub async fn update(&self, id: &Uuid, user: ApiUser) -> Option<()> {
        let mut users = self.users.write().await;
        if users.contains_key(id) {
            users.insert(*id, user);
            Some(())
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub async fn delete(&self, id: &Uuid) -> Option<ApiUser> {
        let user = self.users.write().await.remove(id);
        if let Some(ref u) = user {
            self.email_index.write().await.remove(&u.email);
        }
        user
    }
}

impl Default for UserStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Database-backed user store
#[derive(Clone)]
pub struct DatabaseUserStore {
    store: Arc<oxify_storage::UserStore>,
}

impl DatabaseUserStore {
    pub fn new(pool: oxify_storage::DatabasePool) -> Self {
        Self {
            store: Arc::new(oxify_storage::UserStore::new(pool)),
        }
    }

    pub async fn create(&self, user: ApiUser) -> Result<(), String> {
        // Store base user data
        self.store
            .create(
                user.id,
                user.username.clone(),
                user.email.clone(),
                user.password_hash.clone(),
                user.full_name.clone(),
            )
            .await
            .map_err(|e| e.to_string())?;

        // Store roles
        for role in &user.roles {
            self.store
                .add_role(&user.id, role.clone())
                .await
                .map_err(|e| e.to_string())?;
        }

        // Store permissions
        for permission in &user.permissions {
            let perm_str = format!("{:?}", permission); // Convert Permission enum to string
            self.store
                .add_permission(&user.id, perm_str)
                .await
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    pub async fn get(&self, id: &Uuid) -> Option<ApiUser> {
        let row = self.store.get(id).await.ok()??;
        let id_str = id.to_string();
        let roles = self.store.get_roles(&id_str).await.ok()?;
        let permissions = self.store.get_permissions(&id_str).await.ok()?;

        Some(ApiUser::from_db_row(row, roles, permissions))
    }

    pub async fn get_by_email(&self, email: &str) -> Option<ApiUser> {
        let row = self.store.get_by_email(email).await.ok()??;
        let roles = self.store.get_roles(&row.id).await.ok()?;
        let permissions = self.store.get_permissions(&row.id).await.ok()?;

        Some(ApiUser::from_db_row(row, roles, permissions))
    }

    #[allow(dead_code)]
    pub async fn exists_by_email(&self, email: &str) -> bool {
        self.store.exists_by_email(email).await.unwrap_or(false)
    }

    #[allow(dead_code)]
    pub async fn list(&self) -> Result<Vec<ApiUser>, String> {
        let rows = self.store.list().await.map_err(|e| e.to_string())?;

        let mut users = Vec::new();
        for row in rows {
            let roles = self
                .store
                .get_roles(&row.id)
                .await
                .map_err(|e| e.to_string())?;
            let permissions = self
                .store
                .get_permissions(&row.id)
                .await
                .map_err(|e| e.to_string())?;
            users.push(ApiUser::from_db_row(row, roles, permissions));
        }

        Ok(users)
    }

    #[allow(dead_code)]
    pub async fn update(&self, id: &Uuid, user: ApiUser) -> Option<()> {
        // Verify user exists
        self.store.get(id).await.ok()??;

        // Update full name if provided
        self.store.update_full_name(id, user.full_name).await.ok()?;

        let id_str = id.to_string();

        // Update roles - remove old, add new
        let current_roles = self.store.get_roles(&id_str).await.ok()?;
        for role in current_roles {
            self.store.remove_role(id, &role).await.ok()?;
        }
        for role in &user.roles {
            self.store.add_role(id, role.clone()).await.ok()?;
        }

        // Update permissions - remove old, add new
        let current_perms = self.store.get_permissions(&id_str).await.ok()?;
        for perm in current_perms {
            self.store.remove_permission(id, &perm).await.ok()?;
        }
        for permission in &user.permissions {
            let perm_str = format!("{:?}", permission);
            self.store.add_permission(id, perm_str).await.ok()?;
        }

        Some(())
    }

    #[allow(dead_code)]
    pub async fn delete(&self, id: &Uuid) -> Result<(), String> {
        self.store.delete(id).await.map_err(|e| e.to_string())
    }
}

/// Workflow store backend that supports both in-memory and database storage
#[derive(Clone)]
pub enum WorkflowStoreBackend {
    InMemory(WorkflowStore),
    Database(oxify_storage::WorkflowStore),
}

impl WorkflowStoreBackend {
    pub fn new_in_memory() -> Self {
        Self::InMemory(WorkflowStore::new())
    }

    pub fn new_database(pool: oxify_storage::DatabasePool) -> Self {
        Self::Database(oxify_storage::WorkflowStore::new(pool))
    }

    pub async fn create(&self, workflow: Workflow) -> Result<WorkflowId, String> {
        match self {
            Self::InMemory(store) => Ok(store.create(workflow).await),
            Self::Database(store) => store.create(&workflow).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn get(&self, id: &WorkflowId) -> Result<Option<Workflow>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get(id).await),
            Self::Database(store) => store.get(id).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn list(&self) -> Result<Vec<Workflow>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list().await),
            Self::Database(store) => store.list().await.map_err(|e| e.to_string()),
        }
    }

    pub async fn update(&self, id: &WorkflowId, workflow: Workflow) -> Result<Option<()>, String> {
        match self {
            Self::InMemory(store) => Ok(store.update(id, workflow).await),
            Self::Database(store) => {
                store
                    .update(id, &workflow)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(Some(()))
            }
        }
    }

    pub async fn delete(&self, id: &WorkflowId) -> Result<bool, String> {
        match self {
            Self::InMemory(store) => Ok(store.delete(id).await.is_some()),
            Self::Database(store) => store.delete(id).await.map_err(|e| e.to_string()),
        }
    }
}

/// Execution store backend that supports both in-memory and database storage
#[derive(Clone)]
pub enum ExecutionStoreBackend {
    InMemory(ExecutionStore),
    Database(oxify_storage::ExecutionStore),
}

impl ExecutionStoreBackend {
    pub fn new_in_memory() -> Self {
        Self::InMemory(ExecutionStore::new())
    }

    pub fn new_database(pool: oxify_storage::DatabasePool) -> Self {
        Self::Database(oxify_storage::ExecutionStore::new(pool))
    }

    pub async fn create(&self, execution: ExecutionContext) -> Result<Uuid, String> {
        match self {
            Self::InMemory(store) => Ok(store.create(execution).await),
            Self::Database(store) => store.create(&execution).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn get(&self, id: &Uuid) -> Result<Option<ExecutionContext>, String> {
        match self {
            Self::InMemory(store) => Ok(store.get(id).await),
            Self::Database(store) => store.get(id).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn list(&self) -> Result<Vec<(Uuid, ExecutionContext)>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list().await),
            Self::Database(store) => store.list().await.map_err(|e| e.to_string()),
        }
    }

    pub async fn list_by_workflow(
        &self,
        workflow_id: &WorkflowId,
    ) -> Result<Vec<(Uuid, ExecutionContext)>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list_by_workflow(workflow_id).await),
            Self::Database(store) => store
                .list_by_workflow(workflow_id)
                .await
                .map_err(|e| e.to_string()),
        }
    }

    pub async fn update(
        &self,
        id: &Uuid,
        execution: ExecutionContext,
    ) -> Result<Option<()>, String> {
        match self {
            Self::InMemory(store) => Ok(store.update(id, execution).await),
            Self::Database(store) => {
                store
                    .update(id, &execution)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(Some(()))
            }
        }
    }
}

/// User store backend that supports both in-memory and database storage
#[derive(Clone)]
pub enum UserStoreBackend {
    InMemory(UserStore),
    Database(DatabaseUserStore),
}

impl UserStoreBackend {
    pub fn new_in_memory() -> Self {
        Self::InMemory(UserStore::new())
    }

    pub fn new_database(pool: oxify_storage::DatabasePool) -> Self {
        Self::Database(DatabaseUserStore::new(pool))
    }

    pub async fn create(&self, user: ApiUser) -> Result<(), String> {
        match self {
            Self::InMemory(store) => {
                store.create(user).await;
                Ok(())
            }
            Self::Database(store) => store.create(user).await,
        }
    }

    pub async fn get(&self, id: &Uuid) -> Option<ApiUser> {
        match self {
            Self::InMemory(store) => store.get(id).await,
            Self::Database(store) => store.get(id).await,
        }
    }

    pub async fn get_by_email(&self, email: &str) -> Option<ApiUser> {
        match self {
            Self::InMemory(store) => store.get_by_email(email).await,
            Self::Database(store) => store.get_by_email(email).await,
        }
    }

    #[allow(dead_code)]
    pub async fn list(&self) -> Result<Vec<ApiUser>, String> {
        match self {
            Self::InMemory(store) => Ok(store.list().await),
            Self::Database(store) => store.list().await,
        }
    }

    #[allow(dead_code)]
    pub async fn update(&self, id: &Uuid, user: ApiUser) -> Option<()> {
        match self {
            Self::InMemory(store) => store.update(id, user).await,
            Self::Database(store) => store.update(id, user).await,
        }
    }

    #[allow(dead_code)]
    pub async fn delete(&self, id: &Uuid) -> Result<(), String> {
        match self {
            Self::InMemory(store) => {
                store.delete(id).await;
                Ok(())
            }
            Self::Database(store) => store.delete(id).await,
        }
    }
}
