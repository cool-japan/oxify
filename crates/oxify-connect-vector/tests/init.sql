-- Initialize PostgreSQL database with pgvector extension

-- Create the pgvector extension
CREATE EXTENSION IF NOT EXISTS vector;

-- Grant necessary permissions
GRANT ALL PRIVILEGES ON DATABASE vectordb TO test;
GRANT ALL PRIVILEGES ON SCHEMA public TO test;

-- Create a test table to verify the extension works
CREATE TABLE IF NOT EXISTS test_vectors (
    id SERIAL PRIMARY KEY,
    embedding vector(128),
    metadata JSONB
);

-- Create an index for vector similarity search
CREATE INDEX IF NOT EXISTS test_vectors_embedding_idx
ON test_vectors
USING ivfflat (embedding vector_cosine_ops)
WITH (lists = 100);
