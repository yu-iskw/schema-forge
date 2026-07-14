---
name: parallel-executor
description: Execute complex tasks using parallel subagents. Use for large tasks that benefit from concurrent execution. Automatically plans, decomposes, and orchestrates parallel work.
---

# Parallel Task Execution

## Purpose

This skill invokes the parallel execution system for complex tasks that can benefit from concurrent subagent work.

## Arguments

- `$ARGUMENTS`: Description of the complex task to execute

## Usage

When invoked with `/parallel-executor <task description>`, this skill:

1. Invokes the `parallel-executor` agent to orchestrate the work
2. The agent decomposes the task using `parallel-tasks-planner`
3. Executes independent subtasks in parallel via `task-worker` agents
4. Coordinates sequential phases based on dependencies
5. Verifies results using the `verifier` agent

## Workflow

### Invoke the Parallel Executor Agent

Use the Task tool to invoke the parallel-executor agent:

```text
Task tool call:
  description: "Execute parallel tasks"
  subagent_type: "parallel-executor"
  prompt: |
    Execute the following complex task using parallel decomposition:

    Task: $ARGUMENTS

    Follow your workflow:
    1. Use parallel-tasks-planner to decompose the task
    2. Validate the plan for file conflicts
    3. Execute phases (parallel and sequential)
    4. Verify results
    5. Report summary
```

## Architecture

```text
/parallel-executor "Add logging to all modules"
              │
              ▼
    ┌─────────────────────┐
    │  parallel-executor  │ (orchestrator agent)
    │       agent         │
    └──────────┬──────────┘
               │
               ▼
    ┌─────────────────────┐
    │ parallel-tasks-     │ (planning agent)
    │   planner agent     │
    └──────────┬──────────┘
               │
               ▼ (YAML execution plan)
               │
    ┌──────────┴──────────┐
    │   Phase Execution   │
    │                     │
    │  Phase 1 (Parallel):│
    │  ┌────┐ ┌────┐     │
    │  │W1  │ │W2  │     │  (task-worker agents)
    │  └────┘ └────┘     │
    │                     │
    │  Phase 2 (Sequential):│
    │  ┌────────────┐     │
    │  │    W3      │     │
    │  └────────────┘     │
    └──────────┬──────────┘
               │
               ▼
    ┌─────────────────────┐
    │   verifier agent    │ (verification)
    └─────────────────────┘
```

## When to Use

Use this skill for tasks that:

- Span multiple independent modules or files
- Can be logically decomposed into parallel work
- Would benefit from concurrent execution
- Are large enough to justify the planning overhead

**Good candidates:**

- "Add logging to API, database, and utility modules"
- "Implement CRUD endpoints for users, products, and orders"
- "Add type hints to all modules"
- "Write tests for all service classes"

**Not ideal for:**

- Small, focused changes to a single file
- Tasks with heavy interdependencies
- Quick fixes or simple refactoring

## Example

```bash
/parallel-executor Add comprehensive error handling to all API endpoints
```

This will:

1. **Plan**: Decompose into tasks per endpoint/module
2. **Execute Phase 1**: Add error handling utilities (parallel if independent)
3. **Execute Phase 2**: Update endpoints (parallel per endpoint)
4. **Execute Phase 3**: Add tests (parallel)
5. **Verify**: Run lint and tests
6. **Report**: Summary of all changes

## Related

- **parallel-tasks-planner agent**: Creates execution plans
- **task-worker agent**: Executes individual subtasks
- **verifier agent**: Verifies results
- **parallel-execution-patterns.md**: Reference documentation
