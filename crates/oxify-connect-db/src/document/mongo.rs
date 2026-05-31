use async_trait::async_trait;
use futures::TryStreamExt;
use mongodb::{bson, options::ClientOptions, Client};
use serde_json::Value;

use crate::document::{DocumentStore, MongoConfig};
use crate::errors::{DbError, Result};

pub struct MongoProvider {
    db: mongodb::Database,
}

impl MongoProvider {
    pub async fn new(uri: &str, db_name: &str) -> Result<Self> {
        let client_options = ClientOptions::parse(uri).await?;
        let client = Client::with_options(client_options)?;
        let db = client.database(db_name);
        Ok(Self { db })
    }

    pub async fn from_env() -> Result<Self> {
        let cfg = MongoConfig::from_env()?;
        Self::new(&cfg.uri, &cfg.database).await
    }
}

fn value_to_bson_doc(val: Value) -> Result<bson::Document> {
    bson::to_document(&val).map_err(|e| DbError::Document(format!("bson serialize: {e}")))
}

fn bson_doc_to_value(doc: bson::Document) -> Value {
    bson::from_document::<serde_json::Value>(doc).unwrap_or(Value::Null)
}

#[async_trait]
impl DocumentStore for MongoProvider {
    fn provider_name(&self) -> &str {
        "mongodb"
    }

    async fn insert_one(&self, collection: &str, doc: Value) -> Result<String> {
        let coll = self.db.collection::<bson::Document>(collection);
        let bson_doc = value_to_bson_doc(doc)?;
        let result = coll.insert_one(bson_doc).await?;
        let id_str = match &result.inserted_id {
            bson::Bson::ObjectId(oid) => oid.to_hex(),
            other => other.to_string(),
        };
        Ok(id_str)
    }

    async fn find(
        &self,
        collection: &str,
        filter: Value,
        limit: Option<i64>,
    ) -> Result<Vec<Value>> {
        let coll = self.db.collection::<bson::Document>(collection);
        let filter_doc = value_to_bson_doc(filter)?;

        let mut find_op = coll.find(filter_doc);
        if let Some(n) = limit {
            find_op = find_op.limit(n);
        }

        let mut cursor = find_op.await?;
        let mut results = Vec::new();
        while let Some(doc) = cursor
            .try_next()
            .await
            .map_err(|e| DbError::Document(e.to_string()))?
        {
            results.push(bson_doc_to_value(doc));
        }
        Ok(results)
    }

    async fn update_one(&self, collection: &str, filter: Value, update: Value) -> Result<u64> {
        let coll = self.db.collection::<bson::Document>(collection);
        let filter_doc = value_to_bson_doc(filter)?;
        let update_doc = value_to_bson_doc(update)?;
        let set_doc = bson::doc! { "$set": update_doc };
        let result = coll.update_one(filter_doc, set_doc).await?;
        Ok(result.modified_count)
    }

    async fn delete_one(&self, collection: &str, filter: Value) -> Result<u64> {
        let coll = self.db.collection::<bson::Document>(collection);
        let filter_doc = value_to_bson_doc(filter)?;
        let result = coll.delete_one(filter_doc).await?;
        Ok(result.deleted_count)
    }

    async fn count(&self, collection: &str, filter: Value) -> Result<u64> {
        let coll = self.db.collection::<bson::Document>(collection);
        let filter_doc = value_to_bson_doc(filter)?;
        let count = coll.count_documents(filter_doc).await?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_value_to_bson_doc_object() {
        let val = json!({"key": "value", "number": 42});
        let result = value_to_bson_doc(val);
        assert!(result.is_ok());
        let doc = result.unwrap();
        assert!(doc.contains_key("key"));
        assert!(doc.contains_key("number"));
    }

    #[test]
    fn test_value_to_bson_doc_non_object_fails() {
        let val = json!([1, 2, 3]);
        let result = value_to_bson_doc(val);
        assert!(result.is_err());
    }

    #[test]
    fn test_bson_doc_to_value_roundtrip() {
        let original = json!({"name": "alice", "age": 30});
        let doc = value_to_bson_doc(original.clone()).unwrap();
        let recovered = bson_doc_to_value(doc);
        assert_eq!(recovered["name"], Value::String("alice".to_string()));
    }

    #[tokio::test]
    #[ignore = "Requires database"]
    async fn test_mongo_provider_insert_find() {
        std::env::set_var("MONGODB_URI", "mongodb://localhost:27017");
        std::env::set_var("MONGODB_DATABASE", "oxify_test");
        let provider = MongoProvider::from_env().await.expect("connect failed");
        let doc = json!({"test_key": "test_value"});
        let id = provider
            .insert_one("test_collection", doc)
            .await
            .expect("insert failed");
        assert!(!id.is_empty());

        let results = provider
            .find("test_collection", json!({}), Some(10))
            .await
            .expect("find failed");
        assert!(!results.is_empty());
    }
}
