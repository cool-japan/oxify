# OxiFY Workflow Creation Guide

Welcome to the OxiFY Workflow Creation Guide! This document will walk you through creating, editing, and managing workflows using the OxiFY web UI.

## Table of Contents

1. [Getting Started](#getting-started)
2. [Workflow Basics](#workflow-basics)
3. [Creating Your First Workflow](#creating-your-first-workflow)
4. [Node Types](#node-types)
5. [Connecting Nodes](#connecting-nodes)
6. [Workflow Configuration](#workflow-configuration)
7. [Testing and Debugging](#testing-and-debugging)
8. [Workflow Templates](#workflow-templates)
9. [Advanced Features](#advanced-features)
10. [Keyboard Shortcuts](#keyboard-shortcuts)
11. [Best Practices](#best-practices)
12. [Troubleshooting](#troubleshooting)

---

## Getting Started

### Accessing the Workflow Editor

1. Navigate to the OxiFY UI (default: `http://localhost:3000`)
2. Log in with your credentials
3. Click **"Workflows"** in the sidebar
4. Click the **"+ New Workflow"** button

### UI Overview

The workflow editor consists of:
- **Canvas**: Visual workspace for building your workflow DAG (Directed Acyclic Graph)
- **Node Palette**: Drag-and-drop menu of available node types
- **Properties Panel**: Configure selected nodes
- **Toolbar**: Save, validate, test, and export actions
- **Minimap**: Navigate large workflows

---

## Workflow Basics

### What is a Workflow?

A workflow in OxiFY is a Directed Acyclic Graph (DAG) that defines how data flows through a series of processing nodes. Each node performs a specific task, and edges connect nodes to define the execution order.

### Key Concepts

- **Node**: A processing unit (LLM call, data retrieval, condition check, etc.)
- **Edge**: Connection between nodes that passes data
- **Port**: Input/output connection point on a node
- **Execution**: A single run of a workflow with specific input data

---

## Creating Your First Workflow

### Step 1: Create a New Workflow

1. Click **"+ New Workflow"** from the Workflows page
2. Enter a **name** (e.g., "Customer Support Bot")
3. Add a **description** (optional but recommended)
4. Click **"Create"**

### Step 2: Add Nodes

**Method 1: Drag and Drop**
1. Find the node type in the palette (left sidebar)
2. Drag it onto the canvas
3. Drop it where you want

**Method 2: Quick Add**
1. Right-click on the canvas
2. Select **"Add Node"**
3. Choose node type from the menu

**Method 3: Keyboard Shortcut**
- Press `N` to open the quick add menu
- Type the node name
- Press Enter

### Step 3: Configure Nodes

1. Click on a node to select it
2. The properties panel opens on the right
3. Fill in the required fields:
   - **Node Name**: Descriptive name for this instance
   - **Type-specific settings**: Varies by node type

### Step 4: Connect Nodes

**Method 1: Click and Drag**
1. Click on an output port (right side of a node)
2. Drag to an input port (left side of target node)
3. Release to create the connection

**Method 2: Port Menu**
1. Click on an output port
2. Select target node from the connection menu

### Step 5: Validate and Save

1. Click **"Validate"** in the toolbar
   - Checks for cycles in the graph
   - Verifies all nodes are connected
   - Validates node configurations
2. Fix any validation errors shown
3. Click **"Save"** to persist your workflow

---

## Node Types

### 1. Start Node

**Purpose:** Entry point for workflow execution

**Configuration:**
- Input schema: Define expected input structure
- Default values: Optional fallback values

**Example Use Case:**
```json
{
  "user_message": "string",
  "context": "object"
}
```

---

### 2. LLM Node

**Purpose:** Call a Large Language Model for text generation

**Configuration:**
- **Model**: GPT-4, GPT-3.5, Claude, etc.
- **Temperature**: 0.0 (deterministic) to 2.0 (creative)
- **Max Tokens**: Maximum response length
- **System Prompt**: Instructions for the model
- **User Prompt Template**: Template with variable substitution

**Example Configuration:**
```yaml
model: gpt-4
temperature: 0.7
max_tokens: 500
system_prompt: "You are a helpful customer support agent."
prompt_template: "Customer: {user_message}\n\nRespond professionally:"
```

**Tips:**
- Lower temperature (0.1-0.3) for factual responses
- Higher temperature (0.7-1.0) for creative content
- Use clear, specific prompts for best results

---

### 3. Retriever Node

**Purpose:** Fetch relevant documents from a vector database

**Configuration:**
- **Vector Store**: Connection to your vector DB
- **Query**: Search query (can use templates)
- **Top K**: Number of results to return
- **Score Threshold**: Minimum similarity score

**Example Use Case:**
```yaml
query: "{user_message}"
top_k: 5
score_threshold: 0.7
```

---

### 4. Condition Node

**Purpose:** Branch execution based on a condition

**Configuration:**
- **Condition Expression**: JavaScript or Python expression
- **True Path**: Output port for true condition
- **False Path**: Output port for false condition

**Example:**
```javascript
// Condition expression
input.sentiment === "negative" || input.urgency === "high"
```

**Use Cases:**
- Route to human agent if sentiment is negative
- Use different LLMs based on complexity
- Apply different processing based on input type

---

### 5. Loop Node

**Purpose:** Iterate over a collection of items

**Configuration:**
- **Collection Expression**: Path to array in input data
- **Max Iterations**: Safety limit (default: 100)
- **Loop Variable**: Name for current item

**Example:**
```yaml
collection: "input.documents"
max_iterations: 50
loop_variable: "doc"
```

**Use Case:**
```
For each document in search results:
  1. Summarize document
  2. Extract key points
  3. Combine results
```

---

### 6. Transform Node

**Purpose:** Transform data structure using JavaScript/Python

**Configuration:**
- **Script**: Transformation code
- **Input Schema**: Expected input structure
- **Output Schema**: Result structure

**Example:**
```javascript
// Transform script
return {
  summary: input.documents.map(d => d.content).join('\n\n'),
  count: input.documents.length,
  timestamp: new Date().toISOString()
};
```

---

### 7. API Call Node

**Purpose:** Make HTTP requests to external APIs

**Configuration:**
- **URL**: API endpoint (supports templating)
- **Method**: GET, POST, PUT, DELETE
- **Headers**: Request headers
- **Body**: Request payload (for POST/PUT)

**Example:**
```yaml
url: "https://api.example.com/search"
method: POST
headers:
  Content-Type: application/json
  Authorization: "Bearer ${API_KEY}"
body: |
  {
    "query": "{user_message}",
    "limit": 10
  }
```

---

### 8. Join Node

**Purpose:** Merge outputs from multiple parallel branches

**Configuration:**
- **Merge Strategy**: concat, merge, first-wins
- **Input Ports**: Number of inputs to wait for

**Use Cases:**
- Combine results from multiple LLM calls
- Merge search results from different sources
- Aggregate parallel processing results

---

### 9. End Node

**Purpose:** Terminal node that returns final output

**Configuration:**
- **Output Template**: Format final response
- **Status**: Success, failure, or conditional

**Example:**
```json
{
  "response": "{llm_output}",
  "sources": "{retriever_results}",
  "confidence": "{confidence_score}"
}
```

---

## Connecting Nodes

### Connection Rules

1. **No Cycles**: Workflows must be acyclic (DAG)
2. **Type Compatibility**: Output type must match input type
3. **Single Start**: Workflows should have one start node
4. **At Least One End**: Workflows must have at least one end node

### Visual Feedback

- **Green Edge**: Valid connection
- **Red Edge**: Invalid connection (hover for reason)
- **Dashed Edge**: Conditional connection
- **Thick Edge**: High-priority connection

### Data Flow

Data flows from left to right by default:
```
[Start] → [Retriever] → [LLM] → [Transform] → [End]
```

### Parallel Execution

Nodes can execute in parallel when they don't depend on each other:
```
              → [LLM-Summarize]
             ↗                  ↘
[Retriever] →                    → [Join] → [End]
             ↘                  ↗
              → [LLM-Extract]
```

---

## Workflow Configuration

### Workflow Settings

Click the gear icon to access workflow settings:

**General**
- Name and description
- Tags for organization
- Owner/team assignment

**Execution**
- Timeout (default: 300 seconds)
- Retry policy (max retries, backoff strategy)
- Concurrency limit

**Variables**
- Environment variables
- Secrets (API keys, tokens)
- Global constants

**Triggers**
- Manual execution
- Scheduled (cron expression)
- Webhook endpoint
- Event-driven (on data arrival)

---

## Testing and Debugging

### Test Execution

1. Click **"Test"** in the toolbar
2. Provide sample input data
3. Click **"Run Test"**
4. Monitor execution in real-time

### Real-Time Monitoring

During execution, you'll see:
- **Node Status**: Pending (gray), Running (blue), Success (green), Failed (red)
- **Progress Bar**: Overall completion percentage
- **Current Node**: Highlighted in the DAG
- **Live Logs**: Execution logs in the console

### Debugging Tips

**View Node Output**
- Click on a completed node
- Inspect input/output in the properties panel

**Check Logs**
- Open the logs panel (bottom of screen)
- Filter by level (ERROR, WARN, INFO, DEBUG)
- Search logs for specific messages

**Step-by-Step Execution**
- Enable "Debug Mode" for slower execution
- Pause between nodes to inspect state

---

## Workflow Templates

### Using Templates

Start with a template to save time:

1. Click **"New from Template"**
2. Choose a template:
   - **Blank**: Empty canvas
   - **Simple Chat**: Basic LLM conversation
   - **RAG (Retrieval-Augmented Generation)**: Search + LLM
   - **Agent**: Multi-step reasoning with tool use
   - **Parallel Processing**: Concurrent execution paths
   - **Approval Flow**: Human-in-the-loop workflow

3. Customize the template for your needs

### Template Examples

**RAG Template Structure:**
```
[Start] → [Retriever] → [LLM (with context)] → [Transform] → [End]
```

**Agent Template Structure:**
```
[Start] → [LLM-Plan] → [Loop: Execute Tools] → [LLM-Summarize] → [End]
```

---

## Advanced Features

### 1. Variable Substitution

Use `{variable_name}` in any text field to reference:
- Input data: `{input.user_message}`
- Previous node output: `{retriever.results}`
- Workflow variables: `{env.API_KEY}`

**Example:**
```
Prompt: "Based on these documents: {retriever.top_results}, answer: {input.question}"
```

### 2. Conditional Edges

Create smart routing based on conditions:

```javascript
// Edge condition
output.confidence > 0.8 ? "high_confidence_path" : "review_path"
```

### 3. Error Handling

Configure retry logic and fallbacks:

**Node-Level:**
- Retry count: 3
- Retry delay: 1s, 2s, 4s (exponential backoff)
- Fallback node: If all retries fail, route to fallback

**Workflow-Level:**
- On error: Continue, Stop, or Rollback
- Error notifications: Email, Slack, webhook

### 4. Versioning

OxiFY automatically versions workflows:
- View history in the versions panel
- Compare versions side-by-side
- Rollback to previous version
- Branch from any version

### 5. Import/Export

**Export Workflows:**
- JSON format: Full workflow definition
- YAML format: Human-readable format
- Image: PNG/SVG of workflow diagram

**Import Workflows:**
- Drag and drop JSON/YAML file
- Parse from clipboard
- Import from URL

---

## Keyboard Shortcuts

### General
- `Ctrl/Cmd + S` - Save workflow
- `Ctrl/Cmd + Z` - Undo
- `Ctrl/Cmd + Shift + Z` - Redo
- `Ctrl/Cmd + C` - Copy selected node
- `Ctrl/Cmd + V` - Paste node
- `Ctrl/Cmd + D` - Duplicate selected node
- `Delete` - Delete selected node/edge

### Navigation
- `Arrow Keys` - Move selected node
- `Ctrl/Cmd + Scroll` - Zoom in/out
- `Space + Drag` - Pan canvas
- `F` - Fit all nodes in view
- `Ctrl/Cmd + F` - Find/search

### Editing
- `N` - New node (quick add)
- `Ctrl/Cmd + Enter` - Run/test workflow
- `Escape` - Deselect all
- `A` - Select all
- `H` - Toggle help panel

### View
- `Ctrl/Cmd + +` - Zoom in
- `Ctrl/Cmd + -` - Zoom out
- `Ctrl/Cmd + 0` - Reset zoom
- `M` - Toggle minimap
- `L` - Toggle logs panel

---

## Best Practices

### 1. Naming Conventions

**Workflows:**
- Use descriptive names: "Customer Support RAG" not "Workflow 1"
- Include version in name if needed: "Support RAG v2"

**Nodes:**
- Prefix with type: "llm_summarize", "retrieve_docs"
- Use verb_noun format: "extract_entities", "classify_sentiment"

### 2. Documentation

- Add descriptions to all nodes
- Document expected input/output formats
- Include example values
- Explain complex logic in comments

### 3. Organization

- Group related nodes visually
- Use consistent spacing
- Align nodes horizontally/vertically
- Keep workflows simple (< 20 nodes ideal)

### 4. Error Handling

- Always handle LLM failures
- Set reasonable timeouts
- Add fallback nodes for critical paths
- Log errors for debugging

### 5. Performance

- Maximize parallel execution
- Cache expensive operations
- Set appropriate token limits for LLMs
- Use streaming for long responses

### 6. Security

- Never hardcode API keys in prompts
- Use environment variables for secrets
- Validate all user inputs
- Sanitize LLM outputs before using in code

### 7. Testing

- Test with edge cases
- Use realistic data
- Test error scenarios
- Monitor execution time
- Review logs regularly

---

## Troubleshooting

### Common Issues

**Problem: "Cycle detected in workflow"**
- **Cause:** Nodes form a circular dependency
- **Solution:** Remove the edge that creates the cycle
- **Tip:** Use the validation tool to highlight the cycle

**Problem: "Node validation failed"**
- **Cause:** Missing required fields or invalid configuration
- **Solution:** Check properties panel for red indicators
- **Tip:** Hover over error icon for details

**Problem: "Execution timeout"**
- **Cause:** Workflow takes longer than timeout setting
- **Solution:** Increase timeout in workflow settings
- **Tip:** Optimize slow nodes or split into smaller workflows

**Problem: "LLM returns empty response"**
- **Cause:** Prompt too short, model error, or rate limiting
- **Solution:** Check prompt template, try different model
- **Tip:** Review logs for API error messages

**Problem: "Nodes won't connect"**
- **Cause:** Incompatible port types
- **Solution:** Add a transform node to convert data types
- **Tip:** Check connection error message on hover

**Problem: "Workflow won't save"**
- **Cause:** Validation errors or network issue
- **Solution:** Fix validation errors, check network connection
- **Tip:** Export to JSON as backup before fixing

---

## Example Workflows

### Example 1: Simple Q&A Bot

```
[Start: user_question]
    ↓
[Retriever: search_knowledge_base]
    ↓
[LLM: answer_with_context]
    ↓
[End: return_answer]
```

**Use Case:** Answer customer questions using company knowledge base

---

### Example 2: Sentiment-Based Routing

```
[Start: customer_message]
    ↓
[LLM: analyze_sentiment]
    ↓
[Condition: is_negative?]
    ↓ (true)          ↓ (false)
[Alert: notify]    [LLM: auto_respond]
    ↓                  ↓
[End: human]       [End: automated]
```

**Use Case:** Route negative feedback to humans, auto-respond to positive

---

### Example 3: Multi-Source RAG

```
[Start: query]
    ↓
[Split into 3 parallel retrievers]
    ↓              ↓              ↓
[Docs]         [FAQ]          [API]
    ↓              ↓              ↓
[Join: merge_results]
    ↓
[LLM: synthesize_answer]
    ↓
[Transform: format_response]
    ↓
[End: final_answer]
```

**Use Case:** Search multiple sources and synthesize comprehensive answer

---

## Next Steps

- **Explore Templates**: Try each template to understand different patterns
- **Build Your First Workflow**: Start with a simple use case
- **Join the Community**: Share workflows and learn from others
- **Read API Docs**: Understand HTMX endpoints for advanced integration
- **Contribute**: Help improve OxiFY by contributing workflows and feedback

---

## Additional Resources

- [HTMX API Documentation](./HTMX_API.md) - Technical API reference
- [Node Types Reference](./NODE_TYPES.md) - Detailed node documentation
- [Deployment Guide](./DEPLOYMENT.md) - Production deployment instructions
- [Security Best Practices](./SECURITY.md) - Security guidelines

---

## Support

Need help? Here's how to get support:

1. **Documentation**: Check this guide and API docs first
2. **GitHub Issues**: Report bugs or request features
3. **Community Forum**: Ask questions and share workflows
4. **Discord**: Join real-time chat with the community

Happy workflow building! 🚀
