//! # gRPC Service for Authorization Engine
//!
//! High-performance gRPC API for the ReBAC authorization engine.
//! Provides binary encoding for faster serialization compared to REST/JSON.
//!
//! ## Features
//!
//! - **High Performance**: Binary protocol with minimal overhead
//! - **Streaming Support**: Watch API for real-time tuple updates
//! - **Batch Operations**: Check multiple permissions in one round-trip
//! - **Type Safety**: Auto-generated code from proto definitions
//!
//! ## Example
//!
//! ```rust,no_run
//! use oxify_authz::grpc::AuthzGrpcService;
//! use oxify_authz::HybridRebacEngine;
//! use tonic::transport::Server;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a hybrid engine connected to PostgreSQL
//!     let engine = HybridRebacEngine::new("postgres://localhost/authz").await?;
//!     let service = AuthzGrpcService::new(engine);
//!
//!     let addr = "[::1]:50051".parse()?;
//!     println!("Authorization gRPC server listening on {}", addr);
//!
//!     Server::builder()
//!         .add_service(service.into_service())
//!         .serve(addr)
//!         .await?;
//!
//!     Ok(())
//! }
//! ```

use crate::{
    CheckRequest as EngineCheckRequest, HybridRebacEngine, RelationTuple, Resource, Subject,
};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Request, Response, Status};

// Include the generated proto code
pub mod proto {
    tonic::include_proto!("oxify.authz.v1");
}

use proto::authorization_service_server::{AuthorizationService, AuthorizationServiceServer};
use proto::*;

/// gRPC service implementation for authorization
pub struct AuthzGrpcService {
    engine: Arc<HybridRebacEngine>,
    change_tx: broadcast::Sender<WatchResponse>,
}

impl AuthzGrpcService {
    /// Create a new gRPC service with the given engine
    pub fn new(engine: HybridRebacEngine) -> Self {
        let (change_tx, _) = broadcast::channel(1000);
        Self {
            engine: Arc::new(engine),
            change_tx,
        }
    }

    /// Convert to Tonic service
    pub fn into_service(self) -> AuthorizationServiceServer<Self> {
        AuthorizationServiceServer::new(self)
    }

    /// Notify watchers of a change
    fn notify_change(&self, change_type: i32, tuple: &RelationTuple) {
        let response = WatchResponse {
            change_type,
            tuple: Some(tuple_to_proto(tuple)),
            timestamp: chrono::Utc::now().timestamp(),
        };
        let _ = self.change_tx.send(response);
    }
}

#[async_trait]
impl AuthorizationService for AuthzGrpcService {
    async fn check(
        &self,
        request: Request<CheckRequest>,
    ) -> Result<Response<CheckResponse>, Status> {
        let req = request.into_inner();
        let proto_resource = req
            .resource
            .ok_or_else(|| Status::invalid_argument("resource required"))?;
        let proto_subject = req
            .subject
            .ok_or_else(|| Status::invalid_argument("subject required"))?;

        let start = std::time::Instant::now();

        // Create engine check request
        let check_request = EngineCheckRequest {
            namespace: proto_resource.namespace,
            object_id: proto_resource.object_id,
            relation: req.relation,
            subject: proto_to_subject(proto_subject),
            context: None, // TODO: Map proto context to engine context
        };

        let response = self
            .engine
            .check(check_request)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let latency_us = start.elapsed().as_micros() as i64;

        Ok(Response::new(CheckResponse {
            allowed: response.allowed,
            reason: if response.allowed {
                Some("Permission granted".to_string())
            } else {
                Some("Permission denied".to_string())
            },
            latency_us: Some(latency_us),
        }))
    }

    async fn batch_check(
        &self,
        request: Request<BatchCheckRequest>,
    ) -> Result<Response<BatchCheckResponse>, Status> {
        let req = request.into_inner();
        let start = std::time::Instant::now();

        // Convert proto requests to engine requests
        let check_requests: Vec<EngineCheckRequest> = req
            .checks
            .into_iter()
            .filter_map(|check| {
                let proto_resource = check.resource?;
                let proto_subject = check.subject?;
                Some(EngineCheckRequest {
                    namespace: proto_resource.namespace,
                    object_id: proto_resource.object_id,
                    relation: check.relation,
                    subject: proto_to_subject(proto_subject),
                    context: None,
                })
            })
            .collect();

        let responses = self
            .engine
            .batch_check(&check_requests)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let results: Vec<CheckResponse> = responses
            .into_iter()
            .map(|r| CheckResponse {
                allowed: r.allowed,
                reason: if r.allowed {
                    Some("Permission granted".to_string())
                } else {
                    Some("Permission denied".to_string())
                },
                latency_us: None,
            })
            .collect();

        let total_latency_us = start.elapsed().as_micros() as i64;

        Ok(Response::new(BatchCheckResponse {
            results,
            total_latency_us: Some(total_latency_us),
        }))
    }

    async fn write(
        &self,
        request: Request<WriteRequest>,
    ) -> Result<Response<WriteResponse>, Status> {
        let req = request.into_inner();
        let tuple = proto_to_tuple(
            req.tuple
                .ok_or_else(|| Status::invalid_argument("tuple required"))?,
        );

        match self.engine.write_tuple(tuple.clone()).await {
            Ok(_) => {
                self.notify_change(watch_response::ChangeType::Created as i32, &tuple);
                Ok(Response::new(WriteResponse {
                    success: true,
                    error: None,
                }))
            }
            Err(e) => Ok(Response::new(WriteResponse {
                success: false,
                error: Some(e.to_string()),
            })),
        }
    }

    async fn delete(
        &self,
        request: Request<DeleteRequest>,
    ) -> Result<Response<DeleteResponse>, Status> {
        let req = request.into_inner();
        let tuple = proto_to_tuple(
            req.tuple
                .ok_or_else(|| Status::invalid_argument("tuple required"))?,
        );

        match self.engine.delete_tuple(tuple.clone()).await {
            Ok(_) => {
                self.notify_change(watch_response::ChangeType::Deleted as i32, &tuple);
                Ok(Response::new(DeleteResponse {
                    success: true,
                    error: None,
                }))
            }
            Err(e) => Ok(Response::new(DeleteResponse {
                success: false,
                error: Some(e.to_string()),
            })),
        }
    }

    async fn expand(
        &self,
        request: Request<ExpandRequest>,
    ) -> Result<Response<ExpandResponse>, Status> {
        let req = request.into_inner();
        let proto_resource = req
            .resource
            .ok_or_else(|| Status::invalid_argument("resource required"))?;

        // List all tuples for the object with the specified relation
        let tuples = self
            .engine
            .list_object_tuples(&proto_resource.namespace, &proto_resource.object_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Filter by relation and extract subjects
        let subjects: Vec<_> = tuples
            .into_iter()
            .filter(|t| t.relation == req.relation)
            .map(|t| subject_to_proto(&t.subject))
            .collect();

        let count = subjects.len() as i32;

        Ok(Response::new(ExpandResponse { subjects, count }))
    }

    async fn list_tuples(
        &self,
        request: Request<ListTuplesRequest>,
    ) -> Result<Response<ListTuplesResponse>, Status> {
        let req = request.into_inner();
        let proto_resource = req
            .resource
            .ok_or_else(|| Status::invalid_argument("resource required"))?;

        let tuples = self
            .engine
            .list_object_tuples(&proto_resource.namespace, &proto_resource.object_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Apply limit and pagination
        let limit = req.limit.unwrap_or(100) as usize;
        let offset = req
            .page_token
            .as_ref()
            .and_then(|t| t.parse::<usize>().ok())
            .unwrap_or(0);

        let page_tuples: Vec<_> = tuples
            .iter()
            .skip(offset)
            .take(limit)
            .map(tuple_to_proto)
            .collect();

        let next_page_token = if offset + limit < tuples.len() {
            Some((offset + limit).to_string())
        } else {
            None
        };

        Ok(Response::new(ListTuplesResponse {
            tuples: page_tuples,
            next_page_token,
        }))
    }

    type WatchStream = Pin<Box<dyn Stream<Item = Result<WatchResponse, Status>> + Send>>;

    async fn watch(
        &self,
        request: Request<WatchRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let _req = request.into_inner();
        let rx = self.change_tx.subscribe();

        let stream = BroadcastStream::new(rx)
            .filter_map(|result| async move {
                match result {
                    Ok(response) => Some(Ok(response)),
                    Err(_) => None, // Skip lagged messages
                }
            })
            .boxed();

        Ok(Response::new(stream))
    }
}

// Conversion helpers
fn proto_to_resource(proto: proto::Resource) -> Resource {
    Resource {
        namespace: proto.namespace,
        object_id: proto.object_id,
    }
}

fn proto_to_subject(proto: proto::Subject) -> Subject {
    match proto.subject_type {
        Some(proto::subject::SubjectType::UserId(user_id)) => Subject::User(user_id),
        Some(proto::subject::SubjectType::SubjectSet(set)) => {
            let resource = proto_to_resource(set.resource.expect("SubjectSet must have resource"));
            Subject::UserSet {
                namespace: resource.namespace,
                object_id: resource.object_id,
                relation: set.relation,
            }
        }
        None => Subject::User("unknown".to_string()), // Fallback
    }
}

fn proto_to_tuple(proto: proto::RelationTuple) -> RelationTuple {
    let resource = proto_to_resource(proto.resource.expect("Tuple must have resource"));
    let subject = proto_to_subject(proto.subject.expect("Tuple must have subject"));

    RelationTuple::new(
        resource.namespace,
        proto.relation,
        resource.object_id,
        subject,
    )
}

fn resource_to_proto(resource: &Resource) -> proto::Resource {
    proto::Resource {
        namespace: resource.namespace.clone(),
        object_id: resource.object_id.clone(),
    }
}

fn subject_to_proto(subject: &Subject) -> proto::Subject {
    match subject {
        Subject::User(user_id) => proto::Subject {
            subject_type: Some(proto::subject::SubjectType::UserId(user_id.clone())),
        },
        Subject::UserSet {
            namespace,
            object_id,
            relation,
        } => proto::Subject {
            subject_type: Some(proto::subject::SubjectType::SubjectSet(proto::SubjectSet {
                resource: Some(proto::Resource {
                    namespace: namespace.clone(),
                    object_id: object_id.clone(),
                }),
                relation: relation.clone(),
            })),
        },
    }
}

fn tuple_to_proto(tuple: &RelationTuple) -> proto::RelationTuple {
    let resource = Resource {
        namespace: tuple.namespace.clone(),
        object_id: tuple.object_id.clone(),
    };

    proto::RelationTuple {
        resource: Some(resource_to_proto(&resource)),
        relation: tuple.relation.clone(),
        subject: Some(subject_to_proto(&tuple.subject)),
        tenant_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires PostgreSQL database
    async fn test_grpc_check() {
        let engine = HybridRebacEngine::for_testing().await.unwrap();
        let service = AuthzGrpcService::new(engine);

        // Write a tuple
        let tuple = RelationTuple::new(
            "document",
            "viewer",
            "doc1",
            Subject::User("alice".to_string()),
        );

        let write_req = Request::new(WriteRequest {
            tuple: Some(tuple_to_proto(&tuple)),
        });

        let write_resp = service.write(write_req).await.unwrap().into_inner();
        assert!(write_resp.success);

        // Check permission
        let resource = Resource {
            namespace: "document".to_string(),
            object_id: "doc1".to_string(),
        };

        let check_req = Request::new(CheckRequest {
            resource: Some(resource_to_proto(&resource)),
            relation: "viewer".to_string(),
            subject: Some(subject_to_proto(&Subject::User("alice".to_string()))),
            tenant_id: None,
            context: None,
        });

        let check_resp = service.check(check_req).await.unwrap().into_inner();
        assert!(check_resp.allowed);
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL database
    async fn test_grpc_batch_check() {
        let engine = HybridRebacEngine::for_testing().await.unwrap();
        let service = AuthzGrpcService::new(engine);

        // Write tuples
        let tuples = vec![
            RelationTuple::new(
                "document",
                "viewer",
                "doc1",
                Subject::User("alice".to_string()),
            ),
            RelationTuple::new(
                "document",
                "editor",
                "doc2",
                Subject::User("bob".to_string()),
            ),
        ];

        for tuple in &tuples {
            let write_req = Request::new(WriteRequest {
                tuple: Some(tuple_to_proto(tuple)),
            });
            service.write(write_req).await.unwrap();
        }

        // Batch check
        let batch_req = Request::new(BatchCheckRequest {
            checks: vec![
                CheckRequest {
                    resource: Some(proto::Resource {
                        namespace: "document".to_string(),
                        object_id: "doc1".to_string(),
                    }),
                    relation: "viewer".to_string(),
                    subject: Some(subject_to_proto(&Subject::User("alice".to_string()))),
                    tenant_id: None,
                    context: None,
                },
                CheckRequest {
                    resource: Some(proto::Resource {
                        namespace: "document".to_string(),
                        object_id: "doc2".to_string(),
                    }),
                    relation: "editor".to_string(),
                    subject: Some(subject_to_proto(&Subject::User("bob".to_string()))),
                    tenant_id: None,
                    context: None,
                },
            ],
        });

        let batch_resp = service.batch_check(batch_req).await.unwrap().into_inner();
        assert_eq!(batch_resp.results.len(), 2);
        assert!(batch_resp.results[0].allowed);
        assert!(batch_resp.results[1].allowed);
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL database
    async fn test_grpc_expand() {
        let engine = HybridRebacEngine::for_testing().await.unwrap();
        let service = AuthzGrpcService::new(engine);

        // Write tuples
        let resource = Resource {
            namespace: "document".to_string(),
            object_id: "doc1".to_string(),
        };

        let tuples = vec![
            RelationTuple::new(
                "document",
                "viewer",
                "doc1",
                Subject::User("alice".to_string()),
            ),
            RelationTuple::new(
                "document",
                "viewer",
                "doc1",
                Subject::User("bob".to_string()),
            ),
        ];

        for tuple in &tuples {
            let write_req = Request::new(WriteRequest {
                tuple: Some(tuple_to_proto(tuple)),
            });
            service.write(write_req).await.unwrap();
        }

        // Expand
        let expand_req = Request::new(ExpandRequest {
            resource: Some(resource_to_proto(&resource)),
            relation: "viewer".to_string(),
            tenant_id: None,
        });

        let expand_resp = service.expand(expand_req).await.unwrap().into_inner();
        assert_eq!(expand_resp.count, 2);
    }
}
