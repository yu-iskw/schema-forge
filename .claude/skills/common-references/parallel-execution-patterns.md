# Parallel Execution Patterns

## Overview

This reference documents patterns for executing tasks in parallel using Claude Code's Task tool with background agents.

## Core Concepts

### Task Independence

Tasks are independent when:

- They modify different files
- They don't share mutable state
- Output of one is not input to another

### File Ownership

Each file should be "owned" by exactly one parallel task:

```text
✓ Good: Task A owns src/api.py, Task B owns src/db.py
✗ Bad: Task A and B both modify src/shared.py
```

### Execution Phases

Group tasks into phases:

- **Same phase**: Tasks run in parallel (must be independent)
- **Different phases**: Tasks run sequentially (can have dependencies)

## Patterns

### Pattern 1: Module-Based Parallelism

Split work by module/component:

```text
Phase 1 (Parallel):
├── Agent A: src/api/* (API endpoints)
├── Agent B: src/db/* (Database layer)
└── Agent C: src/utils/* (Utilities)

Phase 2 (Sequential):
└── Agent D: src/main.py (Integration)
```

**Use when**: Adding features that touch isolated modules.

### Pattern 2: Layer-Based Parallelism

Split work by software layer:

```text
Phase 1 (Sequential):
└── Agent A: Implementation (src/)

Phase 2 (Parallel):
├── Agent B: Tests (tests/)
├── Agent C: Documentation (docs/)
└── Agent D: Configuration (config/)
```

**Use when**: Tests/docs/config don't need implementation changes.

### Pattern 3: Feature-Based Parallelism

Split work by independent features:

```text
Phase 1 (Parallel):
├── Agent A: Feature X (all X-related files)
├── Agent B: Feature Y (all Y-related files)
└── Agent C: Feature Z (all Z-related files)

Phase 2 (Sequential):
└── Agent D: Integration tests
```

**Use when**: Features are independent and don't share files.

### Pattern 4: Read-Heavy Parallelism

Parallel analysis, sequential modifications:

```text
Phase 1 (Parallel):
├── Agent A: Analyze module 1 → produce plan
├── Agent B: Analyze module 2 → produce plan
├── Agent C: Analyze module 3 → produce plan

Phase 2 (Sequential):
└── Agent D: Apply all plans (has all context)
```

**Use when**: Need to gather information before making changes.

## Task Tool Usage

### Launching Parallel Tasks

To run tasks in parallel, include multiple Task calls in ONE message:

```text
Message contains:
- Task call 1: run_in_background=true
- Task call 2: run_in_background=true
- Task call 3: run_in_background=true

All three launch simultaneously.
```

### Launching Sequential Tasks

For sequential execution, wait for each task to complete:

```text
Message 1: Task call (foreground, wait for completion)
Message 2: Task call (foreground, wait for completion)
Message 3: Task call (foreground, wait for completion)
```

### Checking Background Task Status

```bash
# View recent output
tail -50 /tmp/claude/.../tasks/{agent_id}.output

# Check if file is still being written
ls -la /tmp/claude/.../tasks/{agent_id}.output
```

## Conflict Prevention

### Pre-Execution Validation

Before launching parallel tasks, verify:

```python
# Pseudo-code
files_by_task = {task_id: task.files for task in parallel_tasks}
all_files = []
for task_id, files in files_by_task.items():
    for file in files:
        if file in all_files:
            raise ConflictError(f"{file} assigned to multiple tasks")
        all_files.append(file)
```

### Runtime Detection

If a worker agent needs a file not assigned:

1. Stop immediately
2. Report the conflict
3. Re-plan with correct file assignments

## Best Practices

### DO

- ✓ Maximize parallelism by identifying independent work
- ✓ Assign clear file ownership to each task
- ✓ Use detailed prompts so agents work independently
- ✓ Verify file boundaries before execution
- ✓ Run verification after all phases complete

### DON'T

- ✗ Assign same file to multiple parallel tasks
- ✗ Assume tasks complete in a specific order
- ✗ Skip the planning phase for complex tasks
- ✗ Ignore task completion reports
- ✗ Mix parallel and sequential tasks in same phase

## Quick Reference

| Scenario                   | Approach                          |
| -------------------------- | --------------------------------- |
| Independent modules        | Parallel in same phase            |
| Shared utility file        | Sequential or single owner        |
| Tests after implementation | Separate phase                    |
| Multiple features          | Parallel if no shared files       |
| Refactoring                | Often sequential (ripple effects) |

## Example: Full Execution Flow

```text
1. User: "Add logging to API, DB, and utils modules"

2. Planner produces:
   Phase 1: [api-logging, db-logging, utils-logging] PARALLEL
   Phase 2: [logging-tests] SEQUENTIAL
   Phase 3: [logging-docs] SEQUENTIAL

3. Executor launches Phase 1:
   - 3 background agents start simultaneously
   - Each modifies only their assigned files
   - Wait for all 3 to complete

4. Executor launches Phase 2:
   - 1 foreground agent writes tests
   - Tests can import from all logged modules

5. Executor launches Phase 3:
   - 1 foreground agent writes docs

6. Verification:
   - make lint && make test
   - Report results to user
```
