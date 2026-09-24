# architecture_fix.md

# AspectShift — Parallel Video Processing Architecture Fix

## Purpose

Patch the current parallel video processing architecture around the real product goal:

> Give AspectShift a scheduler that adapts itself to the machine it's running on, while remaining predictable and stable.

The current benchmark experiments have already answered the critical architectural questions. The implementation should now move from benchmarking toward the production architecture described below.

---

# Stage 1 — Remove Forced Per-Job FFmpeg Threading

## Goal

Stop AspectShift from forcing a low, fixed FFmpeg thread count per video-processing job.

## Decisions

- `-threads 1` is clearly hurting performance.
- FFmpeg `auto` already performs well.
- Forcing a fixed thread count isn't portable across machines.
- AspectShift should not pass `-threads` at all in the normal production plan unless there is a specific future reason to do so.
- FFmpeg should own FFmpeg's internal threading.

## Required behavior

The normal production plan should effectively be:

```text
AspectShift
    └── FFmpeg process
            └── FFmpeg decides its internal threading automatically
```

AspectShift must not try to outsmart FFmpeg by forcing something like:

```text
-threads 1
```

The experiments demonstrated that this can be terrible for performance.

## Implementation direction

- Remove the production behavior that sets `ffmpeg_threads_per_job = 1`.
- Make FFmpeg AUTO the default.
- Do not introduce another fixed per-job FFmpeg thread count as a replacement.
- Do not add machine-specific fixed thread values as the production solution.

## Acceptance criteria

- Production FFmpeg invocations no longer force `-threads 1`.
- FFmpeg is allowed to determine its own internal threading.
- No fixed FFmpeg thread count is introduced as the new default.

---

# Stage 2 — Separate FFmpeg Threading From Process-Level Concurrency

## Goal

Give FFmpeg and AspectShift clearly separated responsibilities.

## Architecture decision

### FFmpeg owns:

- Internal threading for an individual encode/process.

### AspectShift owns:

- Process-level concurrency.
- How many independent FFmpeg processes may run simultaneously.

The architecture should therefore be:

```text
AspectShift Scheduler
        │
        ├── FFmpeg Job 1 → FFmpeg manages its own threads
        ├── FFmpeg Job 2 → FFmpeg manages its own threads
        ├── FFmpeg Job 3 → FFmpeg manages its own threads
        └── ...
```

The scheduler does **not** need to know exactly how many FFmpeg threads each job will consume.

FFmpeg handles that.

AspectShift's responsibility is simply deciding:

> "How many FFmpeg processes can I safely have running simultaneously on this machine?"

## Important rationale

Concurrency of 2 can outperform sequential processing.

However, this does **not** establish a universal magic number for concurrency.

The observed result on the current 6-thread machine must not become a hardcoded architecture rule.

## Acceptance criteria

- Internal FFmpeg threading and process-level scheduling are treated as separate concerns.
- The scheduler controls admission of independent jobs.
- The scheduler does not attempt to micromanage FFmpeg's internal thread allocation.

---

# Stage 3 — Move Toward Adaptive Process-Level Concurrency

## Goal

Make concurrency machine-dependent rather than fixed for everybody.

## Decision

Do **not** immediately make the scheduler try to run "as many videos concurrently as it safely can."

That sounds ideal but is difficult to define safely.

A machine's CPU thread count does not automatically translate into an equivalent number of safe FFmpeg processes because:

- FFmpeg itself may use multiple CPU threads.
- The pipeline may involve scaling.
- The pipeline may involve subtitles.
- The pipeline may involve fonts.
- The pipeline may involve image processing.
- The pipeline may involve audio.
- Disk I/O can also become a bottleneck.

For example, a machine having:

```text
6 CPU threads
```

does not automatically mean:

```text
6 FFmpeg processes
```

## Required scheduler philosophy

The scheduler should be:

- Adaptive.
- Conservative.
- Machine-dependent.
- Predictable.
- Stable.

It should behave differently on different machines without hardcoding one universal concurrency value.

The intended direction is:

```text
Machine resources
       ↓
Adaptive scheduler
       ↓
Conservative safe concurrency
       ↓
Independent FFmpeg processes
       ↓
FFmpeg AUTO internal threading
```

## Important constraint

Do not derive a universal concurrency number from the current 6-thread development machine.

The goal is not to discover whether this particular machine prefers 3 FFmpeg processes over 4.

The goal is to build a scheduler that adapts to the machine on which AspectShift is actually running.

---

# Stage 4 — Preserve and Extend Scheduler Safety Mechanisms

## Goal

Make adaptive scheduling fail-safe.

The central safety rule is:

> If measurements are uncertain: do less concurrency, not more.

## Required fail-safe behavior

### CPU information

If CPU information is weird:

```text
→ use conservative capacity
```

Do not aggressively increase concurrency.

### RAM detection

If RAM detection fails:

```text
→ use conservative capacity
```

Do not assume additional capacity.

### Disk pressure

If disk pressure is high:

```text
→ stop admitting jobs
```

Do not continue increasing workload simply because CPU capacity appears available.

### Resource monitoring

If resource monitoring becomes unavailable:

```text
→ retain a safe ceiling
```

Do not aggressively scale concurrency.

### Workload/resource exhaustion

If a particular workload starts causing resource exhaustion:

```text
→ back off
```

The scheduler should reduce concurrency rather than continuing to push the system.

## Stability principle

The desktop application should prioritize safe and predictable behavior over theoretical maximum throughput.

The target behavior is:

> "I might not be maximally fast on every exotic machine, but I should never melt someone's computer trying to be clever."

## Acceptance criteria

The adaptive scheduler must fail conservatively whenever its measurements or resource assumptions are uncertain.

---

# Stage 5 — Introduce Job Cost Estimation

## Goal

Improve job ordering for both actual throughput and perceived throughput.

## Decision

Use the existing shortest-first idea, but do **not** make file size or duration the universal scheduling metric.

A 500 MB 1080p H.264 file and a 500 MB 4K HEVC file are not necessarily equivalent workloads.

Likewise, file duration alone does not fully describe processing cost.

## Required direction

AspectShift should eventually calculate a **job cost estimate** from information it already knows.

The scheduler can use that estimate to prioritize lower-cost jobs first.

The conceptual model is:

```text
Known job properties
        ↓
Job cost estimate
        ↓
Scheduler ordering
        ↓
Lower-cost jobs first
```

## Important distinction

Do not make the scheduler blindly use:

```text
file size
```

or:

```text
duration
```

as the universal workload metric.

Instead, use a cost estimate that can account for the relevant workload characteristics already known by AspectShift.

## Expected benefit

The purpose is not merely:

> "small files finish first."

The deeper benefit is that the queue does not get stuck waiting for several huge jobs while many trivial jobs could already have completed.

This improves:

- Actual throughput behavior.
- Perceived throughput.
- Time until users see completed results.

That makes job ordering a UX improvement as well as a benchmark optimization.

## Acceptance criteria

- Job ordering has a defined cost-estimation concept.
- Lower-cost jobs can be prioritized.
- File size alone is not treated as a universal workload metric.
- Duration alone is not treated as a universal workload metric.
- The scheduler is designed around workload cost rather than a single simplistic property.

---

# Stage 6 — Preserve AUTO as the Production Invariant

## Goal

Make FFmpeg AUTO the stable production rule while the scheduler controls process admission.

## Architectural invariant

```text
FFmpeg internal threading = AUTO
AspectShift process concurrency = scheduler-controlled
```

The scheduler does not need to know whether FFmpeg chooses:

```text
2
4
8
```

or some other internal threading strategy.

It only controls how many independent jobs are admitted.

## Required production direction

```text
                  ┌── FFmpeg Job 1
                  │      └── FFmpeg AUTO threading
                  │
AspectShift       ├── FFmpeg Job 2
Adaptive          │      └── FFmpeg AUTO threading
Scheduler         │
                  ├── FFmpeg Job 3
                  │      └── FFmpeg AUTO threading
                  │
                  └── ...
```

## Acceptance criteria

- AUTO remains the FFmpeg threading invariant.
- Process-level concurrency remains the scheduler's responsibility.
- No fixed FFmpeg thread count becomes the new production invariant.

---

# Stage 7 — Do Not Continue Benchmarking for the Wrong Question

## Goal

Stop spending implementation effort on benchmark combinations that do not change the architecture.

## Findings already established

The experiments established:

- `-threads 1` is clearly hurting performance.
- FFmpeg AUTO already performs well.
- Concurrency of 2 can outperform sequential processing.
- Forcing a fixed thread count isn't portable across machines.
- A universal magic concurrency number has not been established.

## Architectural conclusion

The critical question:

> Should AspectShift force a low per-job FFmpeg thread count?

Answer:

```text
No.
```

The critical question:

> Should AspectShift let FFmpeg manage its own threading?

Answer:

```text
Yes.
```

The critical question:

> Should concurrency be a machine-dependent scheduler concern instead?

Answer:

```text
Yes.
```

This is enough evidence to move forward.

Do not use the current 6-thread machine to derive a universal production concurrency number.

---

# Stage 8 — Final Production Architecture

The target production architecture is:

```text
                    AspectShift
                        │
                        ▼
              Adaptive Job Scheduler
                        │
             ┌──────────┼──────────┐
             │          │          │
             ▼          ▼          ▼
         FFmpeg Job  FFmpeg Job  FFmpeg Job
             │          │          │
             ▼          ▼          ▼
         AUTO        AUTO        AUTO
        threading   threading   threading
```

The responsibilities are:

```text
AspectShift
    ├── Determines job ordering
    ├── Estimates job cost
    ├── Controls process-level concurrency
    ├── Monitors available resources
    ├── Applies conservative safety limits
    └── Backs off when resource pressure/exhaustion occurs

FFmpeg
    └── Determines internal threading for each individual job
```

## Production principles

1. **FFmpeg owns internal threading.**
2. **AspectShift owns process-level concurrency.**
3. **Concurrency is adaptive and machine-dependent.**
4. **The scheduler remains conservative.**
5. **Uncertain measurements reduce concurrency rather than increase it.**
6. **Resource pressure can stop new job admission.**
7. **Resource exhaustion causes the scheduler to back off.**
8. **Job ordering should eventually use a workload cost estimate.**
9. **Lower-cost jobs should be able to complete sooner rather than waiting behind several huge jobs.**
10. **FFmpeg AUTO is the production invariant.**
11. **Do not hardcode a universal concurrency number based on one development machine.**
12. **Do not reintroduce fixed per-job FFmpeg thread allocation as the solution.**

---

# Implementation Order

Implement the architecture in this order:

## Stage 1
Remove production `ffmpeg_threads_per_job = 1` behavior.

## Stage 2
Ensure FFmpeg is invoked without a forced `-threads` value in the normal production path.

## Stage 3
Keep process-level concurrency under AspectShift's scheduler rather than FFmpeg thread allocation.

## Stage 4
Preserve existing scheduler safety mechanisms and make the scheduler conservative when CPU/RAM/resource measurements are uncertain.

## Stage 5
Move concurrency toward adaptive, machine-dependent process admission rather than a universal fixed value.

## Stage 6
Introduce/shape job cost estimation using information AspectShift already knows, rather than using file size or duration as the universal metric.

## Stage 7
Use lower-cost job ordering to improve both actual and perceived throughput.

## Stage 8
Validate the final architecture against the invariant:

```text
FFmpeg = AUTO threading
AspectShift = adaptive process-level concurrency
Fail-safe = reduce concurrency when uncertain or under pressure
```

---

# Final Engineering Target

The implementation should optimize for the product goal:

> **Give AspectShift a scheduler that adapts itself to the machine it's running on, while remaining predictable and stable.**

Do not optimize the architecture around squeezing another small percentage out of the current benchmark machine.

The desired outcome is a desktop application that may not be maximally fast on every exotic machine, but does not risk melting the user's computer by being overly aggressive.
