# oxify-cli

Command-line interface for OxiFY workflow development and execution.

## Overview

**Status: ✅ Functional - Most features implemented**

CLI tool for:
- Local workflow development and testing
- Workflow validation and analysis
- Scaffolding new workflows with templates
- Cost estimation and performance analysis
- Deployment automation
- Testing framework
- Node type inspection and documentation

## Implemented Commands

### Workflow Management

```bash
# Validate workflow structure
oxify workflow validate <file.json> [--strict]

# Create workflow (parse and validate)
oxify workflow create <file.json>

# Import workflow from file or URL
oxify workflow import <source> [--output file.json] [--validate]

# Export workflow to different format
oxify workflow export <file.json> --output <out.yaml> [--format yaml|json]

# Package workflow with dependencies
oxify workflow package <file.json> --output <package.tar.gz> [--include-deps]

# Create new workflow version
oxify workflow version <file.json> --output <new.json> --bump major|minor|patch --message "Change description"

# Show workflow information
oxify workflow info <file.json>

# Compare two workflows
oxify workflow diff <file1.json> <file2.json> [--verbose]
```

### Local Execution

```bash
# Run workflow locally
oxify run <workflow.json> [--vars key=value] [--output results.json]
```

### Template & Scaffolding

```bash
# Create from template
oxify init <template> [--output file.json]
# Available templates: simple, rag, chain, parallel, conditional, loop, error-handling, agent

# Interactive workflow scaffolding
oxify scaffold [--output file.json]
```

### Analysis & Cost Estimation

```bash
# Analyze workflow for optimization opportunities
oxify analyze <workflow.json> --analysis-type structure|batching|optimize

# Estimate execution costs
oxify cost <workflow.json> [--breakdown]

# Compare costs of multiple workflows
oxify cost --compare workflow1.json workflow2.json workflow3.json
```

### Testing

```bash
# Run workflow tests
oxify test <workflow.json> <test-file.yaml> [--verbose] [--timeout 60]

# Run single test
oxify test <workflow.json> --name "test-name" --input key=value --expect "expected-output"
```

### Visualization

```bash
# Visualize workflow structure
oxify visualize <workflow.json> [--format ascii|dot] [--output diagram.dot]
```

### Node Type Reference

```bash
# List all available node types
oxify nodes list

# Show schema for a node type
oxify nodes schema LLM
oxify nodes schema Retriever

# Show example configuration
oxify nodes example LLM [--format json|yaml]
oxify nodes example Code --format yaml
```

### Workflow Statistics & Analytics

```bash
# Show workflow statistics
oxify stats workflow <workflow.json> [--detailed]

# Compare multiple workflows
oxify stats compare <workflow1.json> <workflow2.json> <workflow3.json>
```

### Shell Completion

```bash
# Generate shell completion scripts
oxify completion bash > /etc/bash_completion.d/oxify
oxify completion zsh > ~/.zsh/completion/_oxify
oxify completion fish > ~/.config/fish/completions/oxify.fish
oxify completion powershell > oxify.ps1
```

### Configuration

```bash
# Initialize config file
oxify config init

# Show current configuration
oxify config show

# Set configuration value
oxify config set <key> <value>

# Unset configuration value
oxify config unset <key>
```

### Deployment (Requires API Server)

```bash
# Deploy workflow
oxify deploy <workflow.json> [--env production]

# Manage schedules
oxify schedule create <workflow-id> --cron "0 0 * * *"
oxify schedule list
oxify schedule pause <schedule-id>

# Manage secrets
oxify secret set <key> <value>
oxify secret list

# Manage checkpoints
oxify checkpoint save <execution-id>
oxify checkpoint restore <checkpoint-id>

# Manage webhooks
oxify webhook create <workflow-id> --url https://api.example.com
oxify webhook list

# Version management
oxify version list <workflow-id>
oxify version compare <v1> <v2>
```

## Future Enhancements

### Planned Features

```bash
# Watch and reload on changes
oxify run <workflow.json> --watch

# Debug workflow execution with breakpoints
oxify run <workflow.json> --debug --breakpoint llm_node

# Lint for best practices
oxify lint <workflow.json>

# Execute remote workflow (requires API server)
oxify execute --id workflow_123 --input '{"query": "test"}'

# Workflow execution logs and monitoring
oxify execution logs <execution-id> [--follow]
oxify execution list [--workflow <id>] [--status running|completed|failed]
oxify execution get <id>

# Dry run mode
oxify dry-run <workflow.json>

# Performance profiling
oxify profile <workflow.json>

# Generate workflow from description (AI-powered)
oxify generate --description "A RAG pipeline for customer support"

```

## Configuration

Config file: `~/.oxify/config.toml`

```toml
[server]
url = "https://api.oxify.io"
api_key = "your_api_key"

[llm]
default_provider = "openai"
openai_api_key = "sk-..."
anthropic_api_key = "sk-ant-..."

[vector]
default_provider = "qdrant"
qdrant_url = "http://localhost:6333"
```

## Workflow Format

Workflows are defined in JSON:

```json
{
  "metadata": {
    "name": "Simple Chat",
    "version": "1.0.0",
    "description": "Basic chatbot workflow"
  },
  "nodes": [
    {
      "id": "start",
      "name": "Start",
      "type": "Start"
    },
    {
      "id": "llm",
      "name": "ChatGPT",
      "type": "LLM",
      "config": {
        "provider": "openai",
        "model": "gpt-4",
        "prompt_template": "{{user_input}}"
      }
    }
  ],
  "edges": [
    {
      "from": "start",
      "to": "llm"
    }
  ]
}
```

## Built-in Templates

Available workflow templates (use with `oxify init <template>`):

- `simple` - Basic LLM workflow
- `rag` - Retrieval-Augmented Generation with vector search
- `chain` - Multi-step LLM processing chain
- `parallel` - Parallel execution example
- `conditional` - Conditional routing with Switch node
- `loop` - Loop/iteration with ForEach
- `error-handling` - Try-Catch-Finally pattern
- `agent` - AI agent with reasoning and tool use

## Testing Framework

Test file format (`test.yaml`):

```yaml
tests:
  - name: "Test basic query"
    workflow: "./workflow.json"
    inputs:
      query: "What is Rust?"
    assertions:
      - path: "result.answer"
        contains: "programming language"
      - path: "result.confidence"
        gt: 0.8
    timeout: 30

  - name: "Test error handling"
    workflow: "./workflow.json"
    inputs:
      invalid_input: true
    expect_error: true
```

Run tests:

```bash
# Run test suite
oxify test <workflow.json> <test.yaml> --verbose

# Run single test
oxify test <workflow.json> --name "Test basic query" \
  --input query="What is Rust?" \
  --expect "programming language"
```

## Feature Status

### Implemented ✅
- [x] Workflow validation with comprehensive error reporting
- [x] Local execution engine integration
- [x] Template-based workflow creation (8 templates)
- [x] Interactive scaffolding with prompts
- [x] Workflow visualization (ASCII art and DOT format)
- [x] Cost estimation and comparison
- [x] Workflow analysis and optimization suggestions
- [x] Workflow statistics and analytics
- [x] Testing framework with assertions
- [x] Import/export with format conversion
- [x] Workflow versioning and diffing
- [x] Workflow packaging (tar.gz/zip)
- [x] Node type documentation and examples
- [x] Configuration management
- [x] Shell completion generation (bash, zsh, fish, powershell)

### In Progress 🚧
- [ ] API server integration for remote execution
- [ ] Execution monitoring and logs
- [ ] Watch mode for live reloading

### Planned 📋
- [ ] Interactive workflow builder (TUI with ratatui)
- [ ] Performance profiling and tracing
- [ ] AI-powered workflow generation
- [ ] Plugin system for custom nodes
- [ ] Workflow migration tools (n8n, Zapier, Airflow)

## See Also

- `oxify-model`: Workflow data structures
- `oxify-engine`: Local execution engine
- `oxify-api`: Remote API server
