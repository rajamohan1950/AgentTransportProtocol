# Layer 5: Fault Tolerance

**Crate:** `atp-fault` | **Tests:** 42

Layer 5 provides comprehensive fault tolerance with circuit breakers, heartbeat monitoring, checkpointing, and poison pill detection.

---

## Design Philosophy

ATP assumes agents **will** fail. The question isn't if, but when and how to recover. Layer 5 provides four mechanisms that work together:

## 1. Circuit Breaker

Prevents cascading failures by cutting off unhealthy agents.

```
         ┌─────────┐
   ──────→  CLOSED  │     Normal operation
         └────┬────┘
              │ failure threshold exceeded
              ▼
         ┌─────────┐
         │  OPEN    │     All requests rejected
         └────┬────┘
              │ timeout expires
              ▼
         ┌──────────┐
         │HALF-OPEN │     Probe request sent
         └────┬────┘
           ┌──┴──┐
         OK    Fail
           │      │
         CLOSED  OPEN
```

**Parameters:**
- Failure threshold: number of consecutive failures before opening
- Reset timeout: how long to wait before probing
- Half-open: sends a single probe request to test recovery

## 2. Heartbeat Monitoring

Detects agent failures in **< 100ms**.

```
Agent A ──── ♥ ♥ ♥ ♥ ♥ ──── Agent B
                         │
                    ♥ ♥ _ _ _ ← missed heartbeats
                         │
                    FAILURE DETECTED (< 100ms)
                         │
                    Reroute to healthy agent
```

**Features:**
- Configurable heartbeat interval
- Miss threshold before declaring failure
- Automatic rerouting to healthy agents via Layer 4

## 3. Checkpoint & Restore

For long-running tasks, progress is checkpointed so that:
- If an agent fails mid-task, work is not lost
- A replacement agent can resume from the last checkpoint
- The system tracks checkpoint IDs for exactly-once semantics

```
Task: [████████░░░░░░░░░░░░] 40%
           │
      Checkpoint saved
           │
      Agent fails!
           │
      New agent resumes from checkpoint
           │
Task: [████████████████████] 100%
```

## 4. Poison Pill Detection

Some inputs cause agents to fail repeatedly. These are **poison pills**.

```
Task X → Agent A fails
Task X → Agent B fails
Task X → Agent C fails
           │
      POISON PILL DETECTED
           │
      Task X quarantined
```

**Features:**
- Tracks failure patterns per task input
- After N failures across different agents, marks input as poison
- Prevents the same bad input from taking down the entire network

## Benchmark Impact

| Scenario | Failures | Recovery |
|----------|----------|----------|
| ATP (full) | 0 | 0ms |
| ATP w/o Fault | 2 | inf |

Without Layer 5, tasks **fail permanently** (infinite recovery time). With it, zero failures across 10,000 tasks.

## Why This Matters

In production multi-agent systems:
- Agents running on different machines **will** have network partitions
- LLM API calls **will** timeout or rate-limit
- Some inputs **will** cause unexpected agent behavior
- Without fault tolerance, a single failure can cascade across the entire system

ATP's Layer 5 ensures **zero task failures** even in hostile conditions.

## Next Steps

- [[Benchmarks]] — See fault tolerance in the ablation study
- [[Architecture Overview]] — How all layers work together
