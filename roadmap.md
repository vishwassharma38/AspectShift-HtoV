# AspectShift-HtoV — Parallel Batch Processing Roadmap

**Project:** AspectShift-HtoV (Rust / Tauri / FFmpeg desktop video converter)
**Current baseline:** v0.1.2, `main`
**Goal:** Move from a single sequential batch-render loop to a bounded, resource-aware, capacity-based scheduler, without rewriting the existing per-job rendering pipeline (`render_single`).
**Revision:** v3 — incorporates the parallel-architecture rework from `architecture_fix.md` (see _Changelog_ below; v2's content remains in the historical stage evidence).

**Governing principle for the whole roadmap:**

> Sequential processing is not a separate fallback implementation. It is the same scheduler operating with `total_capacity = 1`. "Resource-aware job scheduling" is the feature — parallelism is just what that scheduler does when it's safe.

**Target architecture (end state):**

```text
Batch
  │
  ▼
Concurrency Planner
  │
  ├── CPU/RAM budget
  ├── disk budget
  ├── job cost
  └── hardware capability (later, deferred)
  │
  ▼
Resource-aware Scheduler (capacity/semaphore-backed admission)
  │
  ├── Job A ──► process_batch_job()
  ├── Job B ──► process_batch_job()
  └── Job C ──► process_batch_job()
                    │
                    ▼
               render_single()
                    │
                    ▼
                  FFmpeg
```

`total_capacity = 1` → safe sequential execution. `total_capacity = 2–4` → bounded parallel execution. One scheduler, one execution path — not two pipelines. `render_single()` remains the untouched unit of work throughout.

**How to use this document**

- This file is the single source of truth for the effort. Update statuses as stages complete.
- Each **Stage** below is scoped to be turned into one focused implementation prompt for OpenCode. Do not combine stages in one prompt — each has its own dependencies, risks, and validation gate.
- Do not start a stage until its **Prerequisites** are checked off.
- Do not move to the next Phase until every stage's **Validation** in the current phase passes.
- Order matters. The order below is deliberately the safest order, not necessarily the fastest.

---

## Changelog (v1 → v2)

A second technical review surfaced six issues, all addressed in this revision:

1. **Stage 0.1 lock semantics were ambiguous.** A pure input-path lock would _block_ the exact same-source/multi-target concurrency the roadmap wants to enable. Fixed: the lock now explicitly protects the (input, output-target) pair, not the input alone — see revised Stage 0.1.
2. **`-threads N` is a budget hint, not a guarantee.** Actual FFmpeg thread usage varies by codec/filters/decode path. Stage 0.3 and the risk register now say this explicitly, and admission control (not thread flags) is the real oversubscription guard.
3. **The scheduler is now capacity/admission-based, not "N literal worker loops."** Phase 2 was restructured around a shared capacity counter that jobs acquire cost-units from, rather than resizing a fixed pool of workers. This is a strict superset of the old design and makes variable job cost (Stage 3.2) a natural extension instead of a redesign.
4. **"Reduce worker count by one" was unsafe as originally written.** You cannot cleanly kill a worker that's mid-encode just because a _different_ job hit a resource error. Stage 3.4 now reduces future admission capacity only, lets already-admitted jobs run to completion, and retries the failed job once capacity allows.
5. **GPU/hardware acceleration deferral is reaffirmed** — Phase 4 explicitly requires a stable, benchmarked CPU-only baseline first (see new Stage 2.7) before any GPU detection work begins.
6. **Benchmarking was missing.** A new **Stage 2.7** requires measuring real 720p/1080p/4K workloads before treating the Stage 1.3 tier table as final. The hard cap of 4 remains a safety ceiling, but the _actual default_ on a given machine class may end up being 2, and Stage 2.7's findings feed back into Stage 1.3.

## Changelog (v2 → v3) — architecture fix applied

The follow-up review `architecture_fix.md` (2026-09-23) was implemented in full. It tightens the production architecture while keeping the roadmap's capacity-based scheduler model intact:

1. **Production never forces `-threads` anymore (architecture_fix Stages 1/2/6).** The `ffmpeg_threads_per_job` field was removed from `ConcurrencyPlan`; neither `plan_for_batch_start` tracing nor `batch_processor.rs` threads a hint through `process_batch_job` → `ResolvedJob::threads_per_job` → `build_ffmpeg_args`. Batch renders run on FFmpeg's own AUTO threading. The `Option<usize>` capability remains on the type/builder for non-batch and future hardware-specific paths and is `None` on every production batch job — `ResolvedJob::threads_per_job = None` is the production invariant.
2. **Fail-safe conservatism is explicit (Stage 4).** `calculate_safe_concurrency` now pins zero/unknown observations (`logical_cpu_threads == 0`, `available_memory_bytes == 0`) to `Sequential`/capacity 1 — the exact safe result the tier table + RAM gate already converged on — and it is covered by dedicated tests (`zero_cpu_threads_resolves_conservatively`, `zero_available_ram_resolves_conservatively`, `missing_ram_and_cpu_never_yield_parallel`).
3. **Job-cost estimation + lowest-estimated-cost ordering (Stages 6/7).** `scheduler::estimate_job_cost(&BatchJob) -> f64` derives a relative cost purely from the job's *resolved configuration* (platform pixel area clamped to `[0.1, 8.0]`, libvpx/WebM penalty, subtitle pipeline penalty, background/logo/text-overlay/overlay/transform/color-filter effects, `remove_audio` discount, speed-preset map) — never input size/duration and no per-admission probing. The dispatcher admits the lowest-estimated-cost job first (`pop_lowest_cost_job`, stable linear scan, strict `<`, FIFO tie-break), so cheap jobs are never held hostage behind a couple of expensive renders and every prior deterministic FIFO test stays valid.
4. **`total_capacity` stays admission-only.** The scheduler still guards `in_flight_cost <= total_capacity` (incl. Stage 3.4's `admission_ceiling`). The fix removed the *per-job thread* dimension and refined queue ordering only; `classify_job_cost` (Normal = 1, Subtitle/Whisper = 2) is unchanged.
5. **Cleanup accompanying the fix:** the one-shot integration suite (empty `tests/` after its scenarios moved in-suite) and the Stage 2.7 benchmark record file were removed from the tree, and `build.rs` no longer emits the now-targetless `cargo:rustc-link-arg-tests` directive (it made every build fail once `tests/` was empty).

---

## Status Legend

`[ ] not started` · `[~] in progress` · `[x] done` · `[!] blocked`

---

# PART 1 — PRE-IMPLEMENTATION FIXES (Phase 0)

Nothing in Part 2 should begin until every Phase 0 stage is complete and validated. These are correctness/safety fixes that are needed regardless of parallelism, but they become **load-bearing** once multiple jobs run concurrently — if skipped, parallelism will surface them as intermittent, hard-to-reproduce bugs (lock collisions or over-serialization, wrong progress numbers, giant merge-conflict-prone diffs).

## Phase 0 — Safety Preparation

### Stage 0.1 — Fix `ProcessingLock` identity and scope

**Status:** [x]
**File(s):** `src-tauri/src/video/lock.rs`
**Prerequisites:** None. This can start immediately.

**Problem (revised understanding):**
The lock file is currently derived from the input file's stem (e.g. `foo.processing`), not its full canonical path. Two different source files with the same basename in different directories (`C:\Videos\foo.mp4` and `D:\OtherVideos\foo.mp4`) collide on the same lock name today.

The naive fix — hash only the canonical input path — is **not correct**, and would silently defeat one of the goals of this whole effort: when the same source file is queued with three different output targets (`9:16`, `1:1`, `16:9`), those are three legitimate, independent `BatchJob`s that this roadmap explicitly wants to allow running concurrently. A lock keyed on input-path-only would force them to serialize against each other, because they'd all fight over the same lock identity.

**You must first decide, explicitly, what the lock is _for_:**

- If its purpose is "don't let the same _source file_ be read/processed by two operations that could conflict" (e.g. protecting against the source being moved/deleted mid-read, or preventing duplicate identical jobs from being queued twice) — the lock should be scoped to the **(canonical input path, output target/config)** pair, not the input path alone.
- If there's a genuine need to serialize _all_ processing of a given source file regardless of target (e.g. a real correctness constraint tied to probing/thumbnail caching that isn't safe under concurrent reads), that must be identified and named explicitly — don't assume it silently.

**Required change (default recommendation — adjust only if a real source-level constraint is found):**

- Derive the lock identity from a hash of `(canonicalized_absolute_input_path, normalized_output_target_identifier)` — e.g. `SHA-256(canonical_input_path || target_aspect_ratio || relevant_output_config)` → `<hash>.processing`.
- This allows the same source to be processed concurrently for different targets, while still preventing a genuine duplicate (same input _and_ same target queued twice) from double-processing.
- Canonicalize the input path before hashing (resolve symlinks/relative segments) so the same file always maps to the same identity component regardless of how it was referenced.
- Keep the existing lock _mechanism_ (acquire/release/cleanup-on-crash semantics) — only the identity derivation changes.

**Risks:**

- Stale `.processing` files from the old stem-based naming scheme left on disk after upgrade. Add cleanup/migration handling or simply ignore old-format lock files during lookup.
- Do not silently swallow lock acquisition failures — a locked (input, target) pair must still block a true duplicate.
- If probing/thumbnail generation or any shared per-source cache turns out to have a real concurrent-access hazard, that is a _separate_ lock/guard from this one — don't conflate "duplicate job protection" with "shared source-read safety." Flag this explicitly if discovered.

**Validation:**

- Unit test: two files with identical basenames in different directories produce different lock identities.
- Unit test: the _same_ input with two _different_ targets produces two different lock identities (i.e. they are allowed to proceed concurrently).
- Unit test: the same input with the _same_ target queued twice produces the same lock identity (i.e. a genuine duplicate is still blocked).
- Regression test: existing single-job lock/unlock behavior still works exactly as before.

---

### Stage 0.2 — Extract per-job lifecycle out of `batch_processor.rs`

**Status:** [x]
**File(s):** `src-tauri/src/video/batch_processor.rs`
**Prerequisites:** Stage 0.1 complete (lock identity/scope should be correct before you re-shape the code that calls it).

**Problem:**
`start_batch()` currently contains both the batch orchestration loop (pop from `VecDeque`, iterate) _and_ the entire single-job lifecycle (subtitle prep → build `ResolvedJob` → `render_single()` → status update → next). This is a large, monolithic function. Introducing a capacity-based scheduler on top of it as-is would be risky and hard to review.

**Required change:**

- Extract the per-job lifecycle into its own function, e.g.:
  ```rust
  async fn process_batch_job(
      app: &AppHandle,
      state: Arc<Mutex<BatchState>>,
      job: BatchJob,
      token: CancellationToken,
      // ...other existing deps
  ) -> JobOutcome
  ```
- The existing sequential loop in `start_batch()` should now simply call `process_batch_job()` once per iteration. Behavior must be functionally identical to before this refactor — this stage is a pure extraction, not a feature change.
- Do **not** touch `render_single()` itself, `convert.rs`, `filter_builder.rs`, `render_layout.rs`, `preset_adapter.rs`, or `probe.rs`. They stay exactly as-is; `process_batch_job()` just calls into them the same way the old inline code did.

**Risks:**

- Easy to accidentally change behavior while "just" extracting a function (e.g. dropping an error path, changing order of state updates). Diff should be reviewed line-by-line against the original inline logic.
- This is the highest-value regression-testing point in the whole roadmap: if this extraction isn't behavior-identical, every later stage inherits the bug.

**Validation:**

- Run full existing single-video and multi-video-sequential test suite (or manual pass) before/after — outputs, progress events, cancellation, and failure handling must be byte-for-byte/behavior-for-behavior identical.
- No new dependencies on scheduler/capacity concepts introduced yet — this stage should compile and run with the batch still fully sequential.

---

### Stage 0.3 — Introduce FFmpeg per-process thread budget (hint, not guarantee)

**Status:** [x]
**File(s):** `src-tauri/src/video/ffmpeg_args_builder.rs`
**Prerequisites:** None functionally, but do this after 0.2 so it lands on the extracted, reviewable code path.

**Problem:**
FFmpeg argument construction currently does not set a thread budget (`-threads <N>`) for software encoders (`libx264`, `libvpx-vp9`). Today, with one job at a time, an unconstrained FFmpeg process is fine. Once multiple FFmpeg processes can run concurrently, unconstrained threading contributes to CPU oversubscription.

**Important caveat (do not skip):** `-threads N` is a **hint/control knob to FFmpeg's own internal threading**, not a hard OS-level CPU limiter, and not proof that a given job will consume exactly N CPU threads. Actual thread/CPU usage still varies by codec, active filters, decode path, and container. Treat this flag as one input to oversubscription control, not as an exact accounting mechanism — the primary defense against oversubscription is **admission control** (how many jobs are allowed to run concurrently at all — see Phase 2), not precise per-job thread math. Stage 2.7's benchmarking is what actually validates whether the combination of admission limits + thread hints behaves well in practice.

**Required change:**

- Add a `threads_per_job: usize` parameter (or equivalent) to the args builder that emits `-threads <N>` for software encoders.
- For this stage, it's acceptable to hardcode/pass a sane default (e.g. current CPU-derived value or a fixed fallback like 4) — the actual dynamic calculation comes in Phase 1. The goal here is just to make the builder _capable_ of being told a thread budget.
- Leave hardware-encoder argument paths untouched (there aren't any yet — see Phase 4).

**Risks:**

- Over-constraining threads on the _current_ sequential path could slow down the existing single-job experience. Make sure the default when `threads_per_job` is unset/None preserves today's unconstrained behavior, so this stage is a no-op for current users until Phase 1 wires in real values.
- Do not treat a chosen `threads_per_job` value as a substitute for real capacity/admission limits — see caveat above.

**Validation:**

- Encode the same test video with and without `-threads N` set to confirm the flag is applied correctly and doesn't break output correctness.
- Confirm default (unset) behavior is unchanged from pre-stage baseline.

*(v3: this stage's goal — making the builder *capable* of a thread budget — is satisfied and stays in place as a `Option<usize>` capability, but the architecture fix removed every production caller: batch jobs pass `None` and FFmpeg runs on its own AUTO threading. The unset/None behavior this stage deliberately preserved is now the production path.)*

---

### Stage 0.4 — Rework global progress calculation to be multi-job-ready

**Status:** [x]
**File(s):** `src-tauri/src/video/queue.rs` (or wherever `BatchState`'s global progress is computed)
**Prerequisites:** Stage 0.2 complete.

**Problem:**
The current global progress formula assumes exactly one active job (`completed jobs' duration + current job's lifecycle progress`). This assumption must be removed before real concurrency is turned on, or global progress/ETA will be visibly wrong the moment more than one job is active.

**Required change:**

- Change the formula to sum across **all currently active** jobs:
  ```text
  total_processed_duration =
      sum(completed_job.duration)
    + sum(active_job.progress_ratio * active_job.duration)  // for every active job, not just one
  ```
- Keep `FileProgress` (per-job `job_id`, `progress`, `status`, `file_path`, `duration`, `selection`) as the source of truth — do not redesign this data model.
- It's fine to keep a `current_job_id` field on `BatchState` for now, repurposed as "primary/displayed job" for any UI element that still expects a single value — but the actual duration-weighted math must not depend on there being only one active job.

**Risks:**

- Frontend code that reads `current_job_id` and assumes it's the _only_ active job may need a small compatibility shim (see Stage 2.4 for the corresponding frontend-facing change).
- Don't do this stage prematurely before 0.2, or you'll be editing the same sprawling function twice.

**Validation:**

- Unit test the aggregation formula directly with synthetic multi-job progress data (e.g. 3 jobs at 80%/30%/50% of different durations) and confirm the total matches hand-calculated expected duration.
- Confirm single-job sequential batches still report identical progress/ETA to pre-stage baseline (this formula must degrade gracefully to the old one-job case).

---

### Stage 0.5 — Baseline regression test pass (gate before Part 2)

**Status:** [x]
**Prerequisites:** Stages 0.1–0.4 complete.

**Purpose:** Lock in a known-good baseline before any concurrency is introduced, so that any bug found in Part 2 can be attributed to the new scheduler code, not to the Phase 0 refactor.

**Required actions:**

- Run (or manually perform) the full existing test matrix: single video render, multiple videos sequential, cancellation mid-render, one deliberate FFmpeg failure inside a batch, skip-existing-output behavior, and subtitle/Whisper path.
- Confirm outputs and progress events are unchanged from pre-Phase-0 behavior.
- Tag/commit this state clearly (e.g. `pre-parallel-baseline`) so Part 2 stages have a clean rollback point.

**Validation:** All of the above pass with no behavior differences from the original v0.1.2 baseline other than the intended fixes (correct lock scope, extracted function, thread-budget capability present but inert, multi-job-ready progress math that degrades correctly to single-job).

---

# PART 2 — PARALLEL BATCH PROCESSING IMPLEMENTATION

Do not start Part 2 until every Phase 0 stage above is `[x]`.

## Phase 1 — Concurrency Planning Foundation

Goal of this phase: introduce the _types and detection logic_ for a concurrency plan, without yet changing how jobs are actually executed. At the end of Phase 1, the app can compute "how much capacity is available," but the batch loop is still sequential.

### Stage 1.1 — System resource detection

**Status:** [x]
**File(s):** New file `src-tauri/src/video/concurrency.rs`
**Prerequisites:** Phase 0 complete.

**Required change:**

- Create `concurrency.rs` with a function `detect_system_resources() -> ResourceProfile` that reports:
  - logical CPU thread count
  - available (not just total) system RAM
- Keep this read-only and side-effect-free; it should be cheap enough to call once per batch start.

**Risks:** Cross-platform differences in how "available RAM" is reported (Windows/macOS/Linux) — use a well-maintained crate (e.g. `sysinfo`) rather than hand-rolling per-OS queries.

**Validation:** Manually verify reported CPU count and available RAM against OS task manager on at least one target platform.

---

### Stage 1.2 — `ConcurrencyPlan` / capacity types

**Status:** [x]
**File(s):** `src-tauri/src/video/types.rs`, `src-tauri/src/video/concurrency.rs`
**Prerequisites:** Stage 1.1.

**Required change:**

- Add to `types.rs` (or a new module if preferred, but keep it backend-only for now — no UI dependency yet):

  ```rust
  enum ExecutionMode { Sequential, Parallel }

  struct ResourceBudget {
      cpu_threads: usize,
      available_memory_mb: u64,
      // gpu_capacity / encoder_capacity fields added in Phase 4 — leave room but don't implement yet
  }

  struct ConcurrencyPlan {
      mode: ExecutionMode,
      total_capacity: usize,          // capacity in "cost units"; a Normal job costs 1 unit (see Stage 3.2)
      resource_budget: ResourceBudget,
  }
  ```

  (v3: the provisional `ffmpeg_threads_per_job: usize` field from v2 was **removed** in the architecture fix — see the changelog. Production never forces `-threads`; FFmpeg AUTO owns per-process threads. Capacity in the scheduler is process-level, not thread-level.)

- Note the naming: `total_capacity` is expressed in **cost units**, not literal OS threads or literal worker handles. A machine with `total_capacity = 4` can run 4 Normal-cost jobs, or 2 Subtitle-cost jobs (cost 2 each — see Stage 3.2), or any mix that doesn't exceed 4 units in flight. This is what makes the Phase 2 scheduler capacity-based rather than a fixed pool of N literal workers.
- These types should be constructible but not yet consumed by the batch loop.

**Validation:** Types compile; a unit test constructs a `ConcurrencyPlan` manually and asserts field values round-trip correctly.

---

### Stage 1.3 — Conservative concurrency calculation (CPU + RAM tiers, hard cap)

**Status:** [x]
**File(s):** `src-tauri/src/video/concurrency.rs`
**Prerequisites:** Stages 1.1, 1.2.

**Required change:**

- Implement `calculate_safe_concurrency(profile: &ResourceProfile) -> ConcurrencyPlan` using a fixed tier table, not a dynamic formula:

  | Logical CPUs | Initial `total_capacity` |
  | -----------: | -----------------------: |
  |          1–2 |                        1 |
  |          3–4 |                        1 |
  |          5–8 |                        2 |
  |         9–16 |                      2–3 |
  |        17–32 |                      3–4 |
  |          33+ |                        4 |

- **Hard cap: 4.** Do not use `total_capacity = logical_cpu_count`.
- Apply a RAM gate on top of the CPU tier:
  - Available RAM < 4 GB → force `total_capacity = 1` (Sequential mode)
  - Available RAM < 8 GB → cap at 2
  - Available RAM ≥ 8 GB → use the CPU-tier value unmodified
- Compute `ffmpeg_threads_per_job` as: reserve a fixed headroom (e.g. 4 threads) for OS/UI/WebView/Whisper/filesystem, then `floor((cpu_threads - reserved) / total_capacity)`, minimum 1. Remember (Stage 0.3) this is a hint, not an exact guarantee. *(v3: this entire computation was removed — production no longer passes a thread hint to FFmpeg; see the changelog.)*
- If the tier table + RAM gate resolves to `total_capacity <= 1`, set `mode = Sequential`; otherwise `mode = Parallel`.
- **Uncertainty is conservative by default (Stage 4 of the architecture fix):** if a platform observation is missing/zero (`logical_cpu_threads == 0` or `available_memory_bytes == 0`), resolve to `Sequential`/capacity 1 rather than guessing — never let an incomplete read imply parallel capacity.
- **Treat this table as a provisional default, not a final answer.** Stage 2.7 (benchmarking) is explicitly expected to revise these numbers — in particular, the actual best default on many real machines may turn out to be 2, not the ceiling of 4. Leave a clear TODO/comment in the code pointing at Stage 2.7's findings once available.

> **Stage 2.7 benchmark evidence (2026-09-17):** Machine Class A measured;
> see `benchmarks/phase-2-stage-2.7.md` *(v3: the benchmark record file was
> removed from the tree in the architecture-fix cleanup; the conclusions below
> are preserved here as the working record)*. Class A (6 threads, ~4.2 GiB RAM
> available, RAM-gated to capacity 2) showed: capacities 2–3 ≈ sequential,
> capacity 4 was 29–37 % faster for 4-video batches, and sequential
> (`-threads 2`) won the 2-clip 4K cell. On that evidence the table above is
> **explicitly confirmed unchanged for now** — it is conservative (no measured
> default over-allocates: capacity 4 never exceeded the 4-thread headroom on
> this class), but a final decision (notably "should 5–8-thread machines target
> 4 rather than 2?") is deferred until Machine Class B (higher-end) is
> benchmarked. The distinction remains: **recommended default** = tier-derived
> (2 on Class A), **configurable capacity** = the scheduler accepts 1–4,
> **hard safety ceiling** = 4.

**Risks:** Being too aggressive here directly causes the "parallel is slower than sequential" failure mode. Bias conservative — it's easier to raise the cap later after real benchmarking than to walk back a bad first impression.

**Validation:**

- Unit tests covering each tier boundary (e.g. exactly 4, 5, 8, 9, 16, 17, 32, 33 CPUs) and each RAM gate boundary.
- Confirm the hard cap of 4 is never exceeded even on very high core-count machines.
- Confirm low-RAM machines always resolve to `Sequential`.

---

**Phase 1 exit gate:** `ConcurrencyPlan` can be computed correctly for a range of synthetic hardware profiles, but nothing in the actual batch execution path uses it yet. Do not proceed to Phase 2 until this is independently testable and verified via unit tests.

---

## Phase 2 — Capacity-Based Scheduler (the core feature)

Goal: replace the single sequential loop with a **capacity/admission-based scheduler**, not a fixed pool of N literal worker loops. Jobs acquire cost-units of capacity before running; capacity is released when they finish. This runs correctly all the way from `total_capacity = 1` (functionally identical to today) up to `total_capacity = 4`, and it's the same mechanism that later accommodates variable job cost (Stage 3.2) and safe capacity reduction on failure (Stage 3.4) without a redesign.

### Stage 2.1 — Implement the capacity-based scheduler

**Status:** [x]
**File(s):** `src-tauri/src/video/batch_processor.rs` (or a new `scheduler.rs` alongside it — prefer a new file to keep `batch_processor.rs` from growing again)
**Prerequisites:** Phase 1 complete, Stage 0.2's `process_batch_job()` in place.

**Design (important — this replaces the original "N worker loops" idea):**

- Do **not** implement this as N persistent async loops each pulling from a shared queue. Implement it as a single dispatcher that:
  1. Pops the next `BatchJob` from the existing `VecDeque` (in order). *(v3: ordering refined by the architecture fix — the dispatcher now pops the **lowest-estimated-cost** job first via `pop_lowest_cost_job` (stable linear scan, strict `<`, FIFO tie-break); equal-cost jobs keep this in-order behavior, so this bullet's semantics hold unchanged for the deterministic paths.)*
  2. Determines the job's cost (for this stage, before Stage 3.2 lands, every job costs `1` unit — the classification hook can be a stub that always returns `1`).
  3. Acquires `cost` units of capacity from a shared, mutex/atomic-guarded capacity counter (or `tokio::sync::Semaphore` if its acquire/release semantics are sufficient — see the note in Stage 3.4 about why a literal `Semaphore` may need to be supplemented with a separately tracked "current cap" for safe dynamic reduction later). Acquisition should be async and block the dispatcher from pulling more jobs only when capacity is genuinely exhausted — it must not block already-admitted jobs from continuing.
  4. Once capacity is acquired, spawns a `tokio::task` running `process_batch_job()` for that job, and continues the dispatch loop immediately (it does not wait for that job to finish before considering the next one).
  5. When a spawned task completes (success or failure), it releases its `cost` units back to the capacity counter.
  6. The dispatcher loop ends when the queue is empty and all spawned tasks have completed.
- Each spawned task's failure must not affect other tasks — a panicking/erroring job must be caught at the `process_batch_job()` boundary and turned into a `JobOutcome::Failed`, not propagated in a way that poisons the capacity counter or crashes the dispatcher.
- `total_capacity = 1` naturally produces one-job-at-a-time behavior — this is your sequential mode, with **zero separate code path**.

**Risks:** This is the highest-complexity stage in the whole roadmap. Concurrency bugs here are subtle (shared mutable state races, deadlocks on the queue mutex or capacity counter, double-processing of the same job, capacity leaks if a release is missed on an error path). Keep the queue access pattern as close as possible to the existing single-consumer pattern — only the admission gate is new.

**Validation:** Unit/integration test with a mocked `process_batch_job` (no real FFmpeg) that verifies: every queued job is processed exactly once, no job is skipped or duplicated, in-flight cost never exceeds `total_capacity`, and capacity is always fully released after each job (no leaks after many jobs).

---

### Stage 2.2 — Wire batch execution through the scheduler with `total_capacity = 1` (parity check)

**Status:** [x]
**File(s):** `src-tauri/src/video/batch_processor.rs` (scheduler wiring); `src-tauri/build.rs` (fix: link `resource.lib` — Windows Common Controls v6 manifest — into integration-test binaries)
**Prerequisites:** Stage 2.1.

**Required change:**

- Replace the direct sequential loop in `start_batch()` with a call into the new scheduler **forced to `total_capacity = 1`** regardless of what `ConcurrencyPlan` would otherwise compute. This isolates "did the scheduler abstraction introduce a regression" from "does real concurrency itself cause issues."
- This stage should not change any user-visible behavior at all.

**Implementation notes (mapping old loop semantics onto the scheduler):**

- The dispatcher reproduces the old loop's admission bookkeeping: `current_job_id`, `current_job_lifecycle_progress = 0.0`, `job_lifecycle_progress.insert(id, 0.0)` at pop time, and returns early on a cancelled token/status without admitting more jobs.
- Per-job mutable context (`subtitle_cache`, `temp_srt_paths`, `temp_subtitle_font_dirs`) is shared across scheduler invocations via `Arc<tokio::sync::Mutex<…>>`; each `process_fn` invocation locks all three, calls `process_batch_job`, and maps the local `JobOutcome` to `scheduler::JobOutcome`.
- The scheduler does **not** set the terminal batch status; the post-`run_scheduler` block in `start_batch()` still applies `Failed`/`Completed` (guarded by `status == Processing`) and `sanitize_terminal_state`, then emits the final `batch://progress` and cleans up temp subtitle files. A `Cancelled` outcome leaves the scheduler-set `BatchStatus::Cancelled` intact.
- The pre-job `emit_batch_progress` that the old loop emitted right before each job was intentionally dropped; `process_batch_job` emits its own early progress so the emitted sequence is equivalent.

**Risks:** None new — this is a deliberate isolation step. If anything breaks here, it's the scheduler plumbing, not concurrency itself.

**Validation:** Re-run the exact Stage 0.5 baseline test matrix. Results must match the pre-Part-2 baseline exactly (same outputs, same progress events, same cancellation/failure behavior). **Result:** all 7 `stage0_5_regression` matrix tests pass verbatim (sequential guarantee, progress reaching 100%, intermediate progress, cancellation + fresh-batch-after-cancel, invalid-input containment, render-failure → `Failed`, skip-existing byte-for-byte preservation, subtitle export/burn). All 127 unit tests pass; `cargo check --all-targets` clean.

**Environment note (unrelated to Stage 2.2, fixed to run the matrix):** this machine has `System32\comctl32.dll` at v5.82 (no `TaskDialogIndirect`), so any test binary that statically links tao's v6 comctl32 imports without an embedded manifest crashes at load with `0xC0000139`. `tauri_build::build()` links `resource.lib` (which carries the Common Controls v6 manifest) into bin/cdylib targets, but not into integration-test targets; `build.rs` now also emits `cargo:rustc-link-arg-tests` for the same `resource.lib` so test binaries get the manifest. *(v3: that `cargo:rustc-link-arg-tests` emission was removed during the architecture-fix cleanup — once the integration suite was deleted and `tests/` was empty, the directive made `cargo check`/`cargo test` fail repo-wide with an invalid-instruction error under Rust 1.97.1.)*

---

### Stage 2.3 — Enable real concurrency (`total_capacity > 1`)

**Status:** [x] **COMPLETE — validated 2026-09-16**

- **Required change applied:** `total_capacity = 1` override removed; `batch_processor.rs` now computes the Phase 1 `ConcurrencyPlan` once per batch via `concurrency::calculate_safe_concurrency(&concurrency::detect_system_resources())`. `total_capacity` is passed to the Stage 2.1 scheduler. *(v3: as of the architecture fix, `ffmpeg_threads_per_job` is **not** wired through anywhere — the `ConcurrencyPlan` field, the `process_batch_job` → `ResolvedJob::threads_per_job` → `render_single` → `build_ffmpeg_args` chain was removed; see the changelog.)*
- **`convert.rs` deviation from roadmap table:** `render_single` needed the minimal change `build_ffmpeg_args(..., job.threads_per_job)` (previously hardcoded `None`) — required by the "args builder actually wired up" requirement; no behavioural change when `threads_per_job` is `None` (non-batch single-video path). *(v3: the threaded `render_single` wiring was unwired by the architecture fix; only the `None`/AUTO path remains for batch renders.)*
- **Measured on this machine:** 6 logical CPUs / ~15.9 GB RAM → `ConcurrencyPlan { total_capacity: 2 }` (the v2 evidence also recorded `ffmpeg_threads_per_job: 1`; the field no longer exists). No hard-coded counts; plan computed at runtime.
- **Validation results (real FFmpeg, real concurrency observed):**
  - Test B `two_videos_run_concurrently_with_independent_progress` — **PASS** (both videos showed `Processing` simultaneously; `max_processing` reached 2)
  - Test C `twelve_videos_drain_within_capacity_cap` — **PASS** (12 drained cleanly, processing never exceeded capacity 2)
  - Test D `same_source_multiple_targets_run_concurrently_and_duplicate_blocked` — **PASS** (3 targets of one source ran concurrently; true duplicate blocked with `Already processing` from the Stage 0.1 lock)
  - Test E (CPU-tier planner unit tests) / Test F (RAM-gate planner unit tests) — **PASS** (52 `video::concurrency` tests)
  - Stage 0.5 regression: `multiple_videos_process_without_exceeding_capacity` supersedes the old sequential assertion (`processing <= capacity` instead of `processing <= 1`) — **PASS** alongside the other 6 tests
  - Full suite: **128 lib + 7 stage0_5 + 3 concurrency_matrix, all green**; `cargo check --all-targets` clean.
- **Proof of real concurrency:** a `batch://progress` snapshot in the updated regression test captured 2 simultaneous `Processing` jobs — impossible under the Stage 2.2 hard override and why the old `processing <= 1` assertion had to be retired.

**Next stages required before defaults finalisation:** Stage 2.8 (default concurrency/safety analysis) depends on Stage 2.7 (benchmarking) — this stage's plans are still runtime-derived, not a chosen product default.

---

### Stage 2.4 — Multi-job progress: frontend-facing wiring

**Status:** [x]
**File(s):** `src-tauri/src/video/queue.rs` (event emission), frontend batch progress display component
**Prerequisites:** Stage 0.4 (backend math), Stage 2.3 (real concurrent jobs to observe).

**Required change:**

- Confirm `batch://progress` events already carry a queue/list of per-file `FileProgress` (they should, per the existing data model) and that the frontend already renders a list of rows keyed by job.
- The only expected frontend change: what was previously "one Processing row" can now legitimately show as "several Processing rows simultaneously." Confirm the UI doesn't assume a single active row.

**Risks:** Low — this should be a small, mostly cosmetic verification pass rather than a rewrite.

**Validation:** Manually run a batch of 3–4 videos and visually confirm multiple rows show "Processing" simultaneously with independently progressing percentages, and completed/failed rows update independently.

**COMPLETE — verified as a pure confirmation pass (no code change required):**
- `batch://progress` already carries the full per-file list: `BatchProgress.queue: Vec<FileProgress>` (all jobs, each with its own `job_id`, `status`, `progress`, `duration`), emitted by `emit_batch_progress`/`get_batch_status` in `batch_processor.rs` — no architectural change needed.
- Frontend already renders one row per job: `queueItems` in `App.tsx` maps every `batchProgress.queue` entry to a `<div key={job.jobId}>` queue-item, with the row's own status text and independent progress fill (`Math.max(videoProgresses[job.jobId] ?? 0, job.progress)` at the row level).
- No single-active-row assumption anywhere: `video://progress` updates a per-`jobId` map; `batch://file-status` logs per job; `countActiveProcessingJobs` counts *all* processing entries in the queue (used for the refresh-confirmation dialog). `current_job_id` is display-only ("primary/displayed job", per Stage 0.4) and is recomputed by the scheduler when the displayed job finishes.
- Multi-Processing verification at the data level: concurrency-matrix Test B (`two_videos_run_concurrently_with_independent_progress`) records real `batch://progress` snapshots and already asserts `max_processing >= 2` with both jobs reaching their own `Completed`/100% entries (ran on this machine at capacity 2; no SKIP).
- Regression: full suite green — 128 lib + 7 `stage0_5_regression` + 3 `concurrency_matrix`; `tsc && vite build` clean.

---

### Stage 2.5 — Cancellation across concurrent jobs

**Status:** [x] **COMPLETE — verified 2026-09-16**

**File(s):** `src-tauri/src/video/queue.rs`, `batch_processor.rs`/`scheduler.rs`
**Prerequisites:** Stage 2.3.

**Problem:** The existing `CancellationToken` design already propagates to `run_ffmpeg()`, which kills its own child process on cancellation — this part doesn't need to change. What needs tightening is the **state machine**: today, `cancel()` immediately flips state to `Cancelled`. With multiple in-flight jobs, there's a real window between "cancellation requested" and "all active FFmpeg children have actually terminated."

**Required change:**

- Introduce a distinction between "cancellation requested" (token cancelled, dispatcher stops pulling new jobs and stops acquiring new capacity) and "cancellation complete" (all spawned tasks have exited and all child processes are confirmed terminated).
- Keep `status = Processing` (or an explicit intermediate status if the state model allows it) until all in-flight tasks have exited, then transition to `Cancelled`.

**Risks:** A UI that shows "Cancelled" while an FFmpeg process is technically still shutting down could confuse users (e.g. they close the app, orphaning a process). This is the specific bug this stage exists to prevent.

**Validation:** Test K from the matrix below — cancel while 2–4 FFmpeg jobs are active; confirm all child processes are verified terminated before the UI reports `Cancelled`.

**Result:** all 7 `stage0_5_regression` matrix tests pass; all 4 `concurrency_matrix` tests pass (including Test K); 129 unit tests pass; `cargo check --all-targets` clean; `cargo clippy --all-targets` no new warnings on changed code. Two-phase cancellation verified: Phase A (`cancel()`) marks queued jobs only, emits `Processing` snapshot, leaves batch status as-is; Phase B (scheduler post-drain block) sets `Cancelled` only after every JoinSet task has exited. Three-layer token guard covers the admit-loop, pre-render, and render races (Race C). Test 9 (deterministic 4-task ordering test) and Test K (real FFmpeg 2-active-job cancel with `Instant`-ordered event assertions and PowerShell CIM orphan scan) both pass.

---

### Stage 2.6 — Per-job failure isolation validation

**Status:** [x]
**Prerequisites:** Stage 2.3.

**Required change:** No new code expected here if Stage 2.1 was implemented correctly (failure containment is a property of the dispatcher/task boundary, not a separate feature) — this stage is a **dedicated verification pass**, not new implementation, to confirm the existing failure semantics (`failed_jobs` counter, per-job `Failed` status, batch continues) hold up under real concurrency, and that capacity is correctly released even on the failure path (no capacity leaks).

**Validation:** Tests I and J from the matrix below (single failure, and multiple simultaneous failures across different in-flight jobs) — confirm no deadlocks, no capacity leak, and correct final batch tally (`Completed: N, Failed: M`).

**Result:** Tests I and J added to the Phase 2 matrix and passing with real FFmpeg through the real scheduler/dispatcher boundary (machine capacity 2). Failures are injected by pre-occupying a job's resolved output path with a directory so `finalize_temp_output`'s rename fails through the ordinary render error path. Test I (`single_failure_is_isolated_and_capacity_is_released`): 4 jobs, 1 forced failure → `failed_jobs=1`, `completed_jobs=3`, exactly one per-job `Failed` (with reason), three `Completed`, healthy outputs valid, `max_processing ≥ 2`, failing job proven to run Processing → Failed through the real boundary while overlapping other in-flight work, full drain within the bounded 240s wait, no orphaned FFmpeg. Test J (`multiple_simultaneous_failures_no_deadlock_no_capacity_leak`): 6 jobs, 2 forced failures (a, b) → `failed_jobs=2`, `completed_jobs=4`, exactly two per-job `Failed` (only a/b), c–f `Completed` with valid outputs, both failing jobs admitted and Processing, each failing job proven to overlap other in-flight work, `max_processing ≥ 2`, full drain within 300s, no deadlock, no orphaned FFmpeg. Because queue admission order is the nondeterministic completion order of the parallel probe/thumbnail prep tasks, the proofs are expressed as per-job event-interval relationships (order-independent) rather than fixed queue-position ordering. Repeated runs (5× concurrency_matrix, all green) plus full regression: all 6 `concurrency_matrix` tests (B, C, D, I, J, K) and all 7 `stage0_5_regression` tests pass.

---

### Stage 2.7 — Benchmark representative workloads before finalizing defaults

**Status:** [~]
**Prerequisites:** Stages 2.1–2.6 complete and passing.

> **Benchmark record:** ~~`benchmarks/phase-2-stage-2.7.md`~~ *(v3: the record file
> was removed from the tree in the architecture-fix cleanup; raw results,
> methodology, machine specs, capacity 2-vs-4 analysis, and the Stage 1.3
> decision are preserved in the early-Class-A signal below, which stands as the
> working record)*.
>
> **Current state (2026-09-17):** Machine Class A (low/mid-range — 6-thread
> Ryzen 5 3500 desktop) fully benchmarked at capacities 1–4 on the 720p/1080p/
> 4K workloads (3 runs/cell). Machine Class B (higher-end) is **the remaining
> hard blocker**: the current development environment exposes no higher-end
> machine — the only machine available is the Class A Ryzen 5 3500 itself, and
> no higher-end local, remote, or CI benchmark environment is configured
> (`git remote`, WSL, and CI workflows were inspected; none provide a second,
> higher-tier host). Per the Stage 2.7 ground rule, Class B must be *real,
> measured* evidence on genuinely higher-end hardware — it may not be
> simulated, inferred, or reused. Stage 2.7 stays `[~]` and will only be marked
> `[x]` after a higher-end machine is benchmarked and Stage 1.3 defaults are
> finalized on the combined evidence. The benchmark override isolation audit
> (env var bench-only, clamped 1..=4, safe fallback on invalid values, not
> exposed to the frontend or persisted) and the baseline workload scope note
> (standard-profile conversions only) remain valid from the (removed) record file.
>
> **Early Class-A signal (provisional, not final):** on a 6-thread desktop the
> capacity-2 natural plan (RAM-gated here) gave no wall-clock benefit over
> sequential for 4-video batches, capacity 4 was 29–37 % faster for the
> 4-video workloads, and sequential won the 2-clip 4K cell. This does **not**
> yet change the tier table — the Stage 1.3 decision stands as conservative
> and unchanged. *(v3 note: the sequential 4K result was measured with an
> explicit `-threads 2`; production no longer forces `-threads`, so a future
> 4K regression would re-run on FFmpeg AUTO threading.)*

**Why this stage exists:** The Stage 1.3 tier table is a conservative _starting guess_, not a validated default. Before this feature is considered complete, it needs to be checked against real measurements — otherwise you risk shipping a "parallel" feature that's actually slower than sequential on common hardware, or an overly-timid one that never benefits from the extra capacity.

**Required actions:**

- On at least one representative low/mid-range machine and one higher-end machine, benchmark total batch wall-clock time for:
  - A batch of several 720p clips
  - A batch of several 1080p clips
  - A batch of one or two 4K clips
  - each run at `total_capacity = 1, 2, 3, 4`
- Compare total wall-clock time, not just CPU utilization — the goal is **lower total batch time**, not maximum resource usage.
- Record whether `total_capacity = 4` actually beats `total_capacity = 2` on each workload/machine class, or whether contention makes it worse.

**Expected outcome / how findings feed back:**

- Update the Stage 1.3 tier table defaults based on what's actually measured — it is fully expected that the _default_ recommended value ends up being **2** rather than the ceiling of 4 on many machines, while 4 remains available as a hard safety ceiling rather than the default target.
- Use these findings to sanity-check the concurrency defaults (in v2 this bullet targeted the `ffmpeg_threads_per_job` hint value, which no longer exists post-fix — capacity defaults are the only tunable left).
- Document the benchmark results (machine specs, workload, capacity level, wall-clock time) directly in this roadmap or a linked benchmarks file, so future changes to the tier table have a documented basis.

**Validation:** A short written benchmark summary exists, and Stage 1.3's tier table/defaults have been updated (or explicitly confirmed unchanged) based on that summary before Phase 3 begins.

---

**Phase 2 exit gate:** A batch of mixed-size videos processes correctly with real, benchmarked concurrency: correct outputs, correct independent progress, correct cancellation, correct failure isolation with no capacity leaks, no lock collisions. Measured benchmark evidence must demonstrate a meaningful wall-clock benefit for parallelism **on at least some representative workloads**, while identifying workloads where sequential execution remains faster; the selected Stage 1.3 default must be justified by the combined benchmark evidence, and the benchmark must not require that every workload benefit from parallelism. This is the point where parallel processing is "working and validated" — Phase 3 makes it safe under adverse conditions, and Phase 4/5 are enhancements.

---

## Phase 3 — Resource Safety & Job Weighting

Goal: prevent the specific adverse scenarios identified in the analysis (low RAM, subtitle/Whisper multiplication, disk exhaustion, resource-related FFmpeg failures) from turning parallelism into a liability.

### Stage 3.1 — Fresh RAM gating at every batch start + post-failure replanning for subsequent batches

**Status:** [x] — implemented & validated 2026-09-17 (evidence in the note below)
**File(s):** `concurrency.rs`, `batch_processor.rs`, `scheduler.rs`, `tests/stage3_1_ram_gating.rs`
**Prerequisites:** Phase 2 **engineering** complete (Stages 2.1–2.6: capacity-based scheduler, admission/plan at batch start, failure isolation). Formal Phase 2 completion (Stage 2.7 `[~]`) is *not* required here: Stage 3.1 only re-reads the existing RAM gate at every batch start and does not depend on the Class B benchmark outcome. Stage 2.7 remains `[~]` until a Class B machine is actually measured; it is **not** marked complete by this stage.

**Required change:** Confirm the RAM gate from Stage 1.3 is actually being read from live `detect_system_resources()` data at each batch start (not a stale/cached value), since available RAM can change between batches on the same running app instance.

> **Terminology (clarified 2026-09-17):** this stage's "failure-time" dimension means
> **post-failure / subsequent-batch replanning** — a failed batch must not poison or
> permanently alter the resource/capacity state a later batch uses; the *next* batch
> performs its own fresh batch-start detection and planning. It does **not** mean
> re-reading RAM mid-batch to dynamically resize, shrink, or interrupt an already
> admitted, currently-running batch. There is no mid-batch RAM re-planning or
> admission-ceiling change inside a running batch in this stage; that reactive,
> failure-triggered reduction belongs to Stage 3.4.

**Validation:** Start a batch, then artificially reduce available RAM (e.g. by running a memory-consuming process alongside), start a second batch, and confirm the concurrency plan adjusts downward appropriately.

**Status note / evidence (2026-09-17):**

- Freshness is now explicit: `start_batch` resolves its plan through `concurrency::plan_for_batch_start()` at every batch start. That seam performs its own `detect_system_resources()` read per call (a fresh `System` snapshot; no caching/`OnceCell`/`static` on the resource path) and derives a brand-new plan via the single `resolve_batch_start_plan` entry point (Stage 1.3 CPU tier → RAM gate → hard cap). A `tracing` diagnostic records `Batch → available RAM → capacity/mode/threads` per batch. The Stage 2.7 `ASPECTSHIFT_BENCH_TOTAL_CAPACITY` override hook was removed in the benchmark-infrastructure cleanup — the seam is the sole batch-start planning path and no env-var override exists in production code.
- Deterministic coverage (Testing matrix Scenario A/B/C + exactly-one-fresh-detection-per-start + plan independence): unit tests in `concurrency.rs` and integration tests in `tests/stage3_1_ram_gating.rs` — Scenario B (`< 4 GiB → Sequential`, `4–8 GiB → cap 2`) and Scenario C (`5 GiB → 3 GiB → 12 GiB` across consecutive batch starts → `2 → 1 → 2`, both directions) pass.
- Real-pipeline cross-batch: consecutive batches each replan at their *own* start (a capacity override set *between* two batches is honored only by the second), and a batch containing a genuine FFmpeg-process failure leaves no stale plan/capacity behind — the next batch runs at full fresh capacity (Stage 2.6 isolation + Stage 3.1 freshness end to end; also scheduler test `capacity_state_does_not_leak_between_batches`).
- Real-system RAM-pressure validation (`cargo test --test stage3_1_ram_gating -- --ignored --test-threads=1 --nocapture`): baseline 5.3 GiB available → plan capacity 2; a ~2.6 GiB touched working set drove available RAM to 2.7 GiB (< 4 GiB gate) → next-batch plan collapsed to Sequential/capacity 1; after release RAM recovered to 5.4 GiB → plan returned to capacity 2.
- Full suite green: `cargo test` (lib 145, `stage0_5_regression` 7, `concurrency_matrix` 7, `benchmark_stage_2_7` 2 + 1 ignored benchmark, `stage3_1_ram_gating` 6 + 1 ignored pressure test) and `cargo clippy --all-targets` adds no new warnings.

---

### Stage 3.2 — Subtitle/Whisper job cost weighting

**Status:** [x] — implemented & validated 2026-09-17 (evidence in the note below)
**File(s):** `scheduler.rs` (`classify_job_cost`, the Stage 2.1 classification hook — the only job-classification/admission site in the scheduler)
**Prerequisites:** Phase 2 **engineering** complete (Stages 2.1–2.6: capacity-based scheduler, admission/plan at batch start, failure isolation). Formal Stage 2.7 benchmark completion is *not* required for Stage 3.2 because the default `Normal = 1`, `Subtitle/Whisper = 2` weighting is explicitly provisional and subtitle-specific benchmarking is optional. Stage 2.7 remains `[~]` until Class B hardware is actually measured; it is **not** marked complete by this stage.

**Problem:** A job with subtitles enabled isn't one FFmpeg process — it's audio-extraction FFmpeg → Whisper → render FFmpeg. Treating it as equivalent "cost" to a plain render job risks 4 "parallel jobs" turning into 8+ concurrent heavy processes.

**Required change:**

- Implement `classify_job(job) -> JobCost` (replacing the Phase 2 stub): `Normal = 1`, `Subtitle/Whisper = 2` (tune weight based on Stage 2.7-style benchmarking specific to subtitle workloads if time allows).
- Because the scheduler is already capacity/admission-based (Stage 2.1), no new admission-tracking system is needed here — this stage is primarily about plugging an accurate cost value into the existing acquire/release calls.

**Risks:** Low, given Phase 2's design already anticipated this. The main risk is under- or over-estimating subtitle job cost — validate empirically rather than guessing.

**Validation:** Queue a mixed batch (e.g. 2 subtitle jobs + 3 normal jobs) with `total_capacity = 4`, and confirm in-flight cost never exceeds 4 (e.g. 2 subtitle jobs alone should occupy all capacity; a subtitle job plus 2 normal jobs should also reach the cap), while still keeping capacity busy (no idle capacity when eligible lower-cost jobs are waiting).

**Status note / evidence (2026-09-17):**

- `classify_job_cost(&BatchJob) -> usize` in `scheduler.rs` now returns `2` when the job is subtitle/Whisper-enabled and `1` otherwise. The detection is the job's actual configuration, identical to the pipeline's authoritative gate in `batch_processor.rs` (`job.output.effects.export_subtitles_enabled() || job.output.effects.burn_subtitles_enabled()`), which is exactly what routes a job through `prepare_subtitles` → `transcribe_to_segments` (Whisper) → render FFmpeg. No filename/UI/incidental heuristic; no new flag or duplicated subtitle configuration.
- No new admission-tracking system: the existing Stage 2.1 `CapacityGate` accepts an arbitrary per-job cost, so the stage only replaced the stub that fed `acquire_cost`/`release` (permit `Drop`). There are **no** other job-cost call sites — the dispatcher in `run_scheduler` is the single classification→acquire point, and release happens exactly once via the RAII `CapacityPermit`.
- Correct cost accounting with `total_capacity = 4` (scheduler unit tests through the real `run_scheduler` admission path):
  - Scenario A — 2 subtitle jobs (`2 + 2 = 4`): both admitted concurrently, observed in-flight cost reaches 4 and never exceeds it.
  - Scenario B — subtitle + 2 normal jobs (`2 + 1 + 1 = 4`): reaches the cap, never exceeds it.
  - Scenario C — 2 subtitle + 3 normal jobs: `in_flight_cost <= 4` at every point, all 5 jobs drain.
  - Lower-cost jobs use available capacity: subtitle (2) + two normals (1+1) fill capacity 4 while the fourth job waits — no head-of-line blocking leaves capacity idle.
- Lifecycle/release: subtitle job acquired = 2; released exactly once (verified `in_flight_cost = 0`, `available_capacity = 4` after the batch) on completion, on failure, on cancellation, and on a task-boundary panic (RAII permit).
- Weight tuning (Stage 2.7-style subtitle benchmarking) was reviewed and **not** run: it is optional in the roadmap, Stage 2.7 is still `[~]` pending a Class B machine, and a meaningful subtitle measurement requires a real Whisper inference workload (the regression stub is not representative). Per the roadmap's required default, **Normal = 1, Subtitle/Whisper = 2** is retained.
- Test totals: `cargo test` — lib **159** (145 prior + 14 new Stage 3.2: 6 classification, 4 capacity scenarios, 4 lifecycle/release), `stage0_5_regression` **7** (incl. `subtitles_export_and_burn_with_stubbed_whisper`), `concurrency_matrix` **7**, `benchmark_stage_2_7` **2** (+1 ignored), `stage3_1_ram_gating` **6** (+1 ignored). `cargo check --all-targets` clean; `cargo clippy --all-targets` introduces **no new warnings** on changed code.

---

### Stage 3.3 — Disk space gating

**Status:** [x]
**File(s):** `concurrency.rs`, job admission path
**Prerequisites:** Phase 2 **engineering** complete (Stages 2.1–2.6: capacity-based scheduler, admission/plan at batch start, failure isolation). Formal Phase 2 / Stage 2.7 benchmark completion is *not* required because disk gating is a safety mechanism independent of the benchmark-finalized concurrency defaults: Stage 3.3 only needs the scheduler/admission infrastructure (a dispatcher that pops one job at a time and a capacity gate to acquire from) that Stages 2.1–2.6 already provide. Stage 2.7 remains `[~]` until a Class B machine is actually measured; it is **not** marked complete by this stage. Independent of Stages 3.1/3.2 — can be done in parallel with those if desired.

**Problem:** Parallel encoding means multiple simultaneous temporary output files. Low disk space combined with concurrency risks silent partial-write failures.

**Required change:**

- Add an `available_disk_space` check (implemented intentionally as a **live per-admission query through `DiskAdmissionGate`**, not a cached `ResourceProfile` field — this is a deliberate design refinement; see the design note below).
- Before the dispatcher acquires capacity for a _new_ job, check free space against a conservative safety margin (exact estimation of required space is out of scope — a simple "healthy margin" threshold is sufficient for v1).
- If space drops critically low mid-batch: stop admitting new jobs, let already-admitted jobs finish if possible, surface a clear "disk space" error, and do **not** auto-retry until space is recovered.

**Validation:** Test L from the matrix below — simulate low/insufficient disk space and confirm: no corrupted final output files, temporary files are cleaned up, a clear error is surfaced, and no infinite retry loop occurs.

> **Intentional design note — disk availability is not cached in `ResourceProfile`**
> (`available_disk_space` deviation, clarified 2026-09-17): CPU/RAM-style
> resource information may remain in the resource profile, but disk availability
> is intentionally **not** cached there. Disk space is a dynamic,
> output-volume-specific resource: it can change substantially while earlier
> jobs in a long-running batch are still writing their output, and it differs
> per output filesystem. Stage 3.3 therefore queries the **current** available
> space live through `DiskAdmissionGate` immediately before each new job is
> admitted, against the volume that will host *that job's* actual output. The
> live check is specifically required because a batch-start snapshot would go
> stale as soon as the first parallel writes begin; probing per admission avoids
> stale disk information during long-running batches. This is an intended
> architectural refinement, **not** an unresolved compliance deviation.

> **Batch-wide disk-pressure behavior (v1, intentional, clarified 2026-09-17):**
> disk availability is still evaluated **per job / per output volume** — each
> admission probes the volume of that job's resolved output, so a `C:\` job and
> a `D:\` job can legitimately receive different verdicts. However, once the
> disk gate blocks an admission, v1 stops admitting **every** remaining job in
> the current batch rather than continuing the queue on other volumes.
> Already-admitted/running jobs are left to finish normally; the popped job and
> all remaining queued jobs are failed with the existing failure semantics (the
> identical clear disk-space reason). This batch-wide stop is a deliberate
> conservative safety policy for v1; per-volume queue continuation (e.g. letting
> `D:\` jobs proceed while `C:\` is full) is intentionally deferred to a future
> improvement.

> **Probe failure vs. genuine low disk (distinct, both fail closed, verified
> 2026-09-17):** the two blocking verdicts are never conflated. A genuine
> low-disk block reports the measured free bytes versus the required margin on
> the named volume (`insufficient free disk space on <volume>: <free> bytes free
> is below the <margin> byte safety margin`). A probe failure reports that disk
> availability **could not be determined** on the named volume (with the probe
> error preserved) and that admission was stopped for safety — no fabricated
> free-space value is ever substituted, and a probe failure is never reported as
> though the disk were definitely full. Both paths stop admission (fail closed).

**Result:** The disk-space check is implemented as a **live per-admission query** (`DiskSpaceSource` + `DiskAdmissionGate`, `DISK_SAFETY_MARGIN_BYTES` = 2 GiB) rather than a field baked into the cached resource profile: the targeted spot for the check is the output volume of each individual job, and a live read at admission is strictly fresher than a batch-start snapshot. `SystemDiskSpaceSource` (sysinfo's `Disks` API — no new dependency) matches the job's output path to its volume by longest mount-point prefix (case-insensitive on Windows); any probe failure **fails closed**. The scheduler checks the gate **before** acquiring capacity for each popped job (`scheduler.rs` dispatcher loop); on a blocking verdict the popped job and all remaining queued jobs reach a terminal `Failed` with an identical message naming the gate-tripping job, its path, free bytes, and the margin, `failed_jobs` is incremented per job, `current_job_id` is recomputed away, and the dispatcher `break`s — no capacity is ever acquired, already-admitted jobs finish normally, and nothing is ever retried. `convert.rs::finalize_temp_output` now also removes the temp artifact when the final-directory rename fails, so a completed render can never be stranded as a full-length `.tmp.*` file. DI: `BatchManager` gained `disk_source` (defaults to the system source; `BatchManager::with_disk_source` injects fakes; benchmarks/`start_batch` call sites untouched; scheduler tests pass a `healthy_disk_gate()`).

- Test totals: `cargo test` — lib **172** (159 prior + 13 new: 8 `concurrency` disk-gate/mount tests + 5 `scheduler` Stage 3.3 scenarios: at-start block, mid-batch exhaustion, fail-closed probe, failed-batch-terminates/recovered-batch-runs-once, and the low-space-vs-probe-failure message-distinction test that proves the two blocking verdicts stay accurately classified). New integration file `stage3_3_disk_gating` **5** (Test A healthy-system-source batch, Test B low-at-start, Test C mid-batch, Test D rename-strand temp cleanup, Test L full matrix walk: clear "disk space" error on every blocked job, all blocked jobs terminal, zero corrupt/corrupted finals, zero `.tmp.*` leftovers, exactly one live probe per admission pop, each admitted job `Processing` exactly once, bounded prompt termination, no retry spin). Regression: `concurrency_matrix` **7**, `stage0_5_regression` **7**, `stage3_1_ram_gating` **6** (+1 ignored), `benchmark_stage_2_7` **2** (+1 ignored). `cargo check --all-targets` clean; `cargo clippy --all-targets` introduces **no new warnings** on changed code (queue.rs gained `impl Default for BatchManager`; remaining clippy warnings are pre-existing in untouched files).

> **Follow-up hardening pass (2026-09-17):** the implementation was re-verified
> against the live code, not just the audit. Confirmed (unchanged, no rewrite):
> disk gate runs **before** capacity acquisition, one live probe per admission
> pop (scripted probe-count tests), output-volume-aware selection via longest
> mount-point prefix with Windows case folding, the strict `2 GiB` boundary
> (`> margin` allows, `<= margin` blocks), fail-closed behavior on probe error,
> temp-output cleanup on failed finalization, and batch-wide stop-on-first-block
> semantics with already-admitted jobs left to finish (per-volume continuation
> deferred). The only code change was a test that proves genuine low disk and
> probe failure produce distinct, accurate user-facing messages. Roadmap
> prerequisites and the intentional `ResourceProfile` / live-query + batch-wide
> designs are documented above; Stage 3.3 remains `[x]`.

---

### Stage 3.4 — Failure classification + safe capacity reduction and retry

**Status:** [x]
**File(s):** wherever `JobOutcome::Failed` is produced/consumed (likely `batch_processor.rs`/`scheduler.rs`)
**Prerequisites:** Stages 3.1–3.3.

**Required change:**

- Classify failures into two buckets:
  - **Deterministic** (invalid input, bad filter, unsupported codec, missing font, invalid output path) → do not retry.
  - **Resource-related** (encoder init failure, out-of-memory, device busy, resource unavailable) → eligible for a one-time reduced-capacity retry.
- **On a resource-related failure, do not kill or interrupt any already-admitted, currently-running job.** The safe sequence is:
  1. Immediately reduce the capacity counter's _future admission_ ceiling by one cost unit (e.g. `total_capacity` effectively drops from 3 to 2 for jobs not yet admitted) — this does not touch jobs already holding capacity.
  2. Let all currently in-flight jobs continue running and finish normally under their already-acquired capacity.
  3. Once capacity allows under the new, reduced ceiling, retry the specific failed job.
  4. If it fails again for the same resource-related reason, reduce the ceiling again (toward `total_capacity = 1`) before giving up and marking it permanently `Failed`.
- Implementation note: if using `tokio::sync::Semaphore` for the capacity mechanism from Stage 2.1, a literal `Semaphore`'s permit count isn't trivially shrunk below currently-issued permits. Either track a separate "current admission ceiling" value that the dispatcher checks _before_ calling `acquire` (simplest), or use an equivalent custom counter guarded by a mutex/atomic. Pick whichever keeps Stage 2.1's dispatcher logic simplest — this is a known implementation nuance to watch for, not a blocker.

**Risks:** Misclassifying a deterministic error as resource-related would waste time retrying something that will never succeed. Keep the classification list conservative and easy to extend rather than trying to be exhaustive on the first pass.

**Validation:** Deliberately trigger one deterministic failure (e.g. malformed input) and confirm no retry occurs and no capacity ceiling change happens. Deliberately simulate a resource-related failure (as best as can be forced in a test environment) and confirm: (a) other already-running jobs are unaffected and finish normally, (b) the admission ceiling for _new_ jobs drops, and (c) the failed job is retried once reduced capacity is available.

> **Status note / evidence (2026-09-17):**
>
> - **Classification** (`FailureClass { Deterministic, ResourceRelated }`,
>   `classify_video_error(&VideoError)`, `is_resource_exhaustion_stderr`)
>   lives in `scheduler.rs`. The classifier is deliberately conservative: only
>   `VideoError::ProcessingFailed { stderr }` whose stderr matches well-known
>   resource-exhaustion messages (cannot/out of/no memory, device or resource
>   busy / device busy, no space left on device, or "could not open codec"
>   combined with resource/memory/busy) is `ResourceRelated`; **every other**
>   error variant — including ambiguous `ProcessingFailed` text and all
>   Whisper/spawn/config errors — defaults to `Deterministic`. Both fixture
>   sets are unit-tested (known-resource stderr is `ResourceRelated`; ambiguous
>   stderr and every other variant are `Deterministic`, e.g. a
>   `WhisperFailed` that mentions "out of memory" is still `Deterministic`).
>   This directly addresses the roadmap risk: no deterministic error is ever
>   mistaken for retryable.
> - **Where classification happens:** all four task-boundary failure sites in
>   `batch_processor.rs` (subtitle orientation probe, resolved plan creation,
>   subtitle preparation, and `render_single`) classify the `VideoError` before
>   recording the per-job failure. A `ResourceRelated` failure records the
>   `Failed` status + emits `batch://file-status` for the frontend and defers the
>   **batch-level** accounting to the scheduler by returning
>   `JobOutcome::ClassifiedFailed { message, failure_class }`; a
>   `Deterministic` failure keeps the pre-existing accounting (increments
>   `failed_jobs`, marks the job terminal) and returns `JobOutcome::Processed`.
>   The processor emits exactly the same `JobStatus::Failed(...)` event in both
>   cases, so the UI never distinguishes a retryable failure from a terminal one.
> - **Ceiling mechanism (roadmap's "watch for" nuance):** the Stage 2.1
>   `CapacityGate` keeps its original `total_capacity`/`available`/`in_flight`
>   atomics as the configured baseline, and gains a **separate
>   `admission_ceiling`** atomic that starts equal to `total_capacity` and is
>   only ever reduced. The dispatcher's `acquire_cost` additionally requires
>   `in_flight + cost <= admission_ceiling` **before** acquiring; a drop in the
>   ceiling therefore never disturbs permits already issued and never breaks
>   the `in_flight_cost <= total_capacity` invariant. This is exactly the
>   "separate current admission ceiling checked before `acquire`" option the
>   roadmap names, and it needs no `Semaphore` shrink trickery.
> - **Retry flow in `run_scheduler`:** the scheduler's join loop classifies each
>   `ClassifiedFailed`:
>   - `Deterministic` → permanently `Failed`, `failed_jobs += 1`, ceiling
>     unchanged, no retry.
>   - `ResourceRelated` first time → `reduce_ceiling_for_failure(1)` (floor 1),
>     record the job id in a `HashSet<String>` retry ledger, reset the job to
>     `Queued`, push it back onto the queue; then the **outer loop** re-enters
>     admission only after the join loop has let every already-admitted job
>     finish normally, and the retry is admitted under the reduced ceiling
>     through the normal gate (disk gate + `acquire_cost`) — it waits when
>     capacity is exhausted.
>   - `ResourceRelated` second time → `reduce_ceiling_for_failure(1)` **again**
>     toward the floor of 1, then permanently `Failed`, `failed_jobs += 1`,
>     **no second retry**.
>   - Deadlock guard: a `ResourceRelated` job whose own cost now exceeds the
>     reduced ceiling cannot be re-admitted, so on the first failure it is
>     permanently failed instead of re-enqueued; the dispatcher also
>     fails-at-pop any queued job whose cost exceeds the current ceiling rather
>     than leaving it blocked forever. Both paths are tested.
> - **Never interrupts running work:** nothing already admitted is cancelled or
>   pre-empted — the ceiling only constrains future admission. The scheduler's
>   summary now reports `admission_ceiling` alongside the unchanged
>   `total_capacity`.
> - **Test totals:** `cargo test` — lib **179** (172 prior + 7 new Stage 3.4: 2
>   conservative-classifier fixtures + 5 scheduler scenarios: deterministic
>   failure is not retried and the ceiling is unchanged; a resource failure
>   reduces the ceiling by one and the single retry succeeds (not counted as a
>   failure); a second resource failure reduces the ceiling again (3→2→1) and
>   permanently fails with exactly one retry; a barrier/watch-based scenario
>   proving job-0 and job-1 keep running while job-2 fails and that job-2's
>   retry is only admitted after an in-flight job finishes (never while they
>   hold capacity under the reduced ceiling); and the cost-2-job / ceiling-1
>   deadlock guard). Integration targets unchanged and green
>   (`stage3_3_disk_gating` **5**, `concurrency_matrix` **7**,
>   `stage0_5_regression` **7**, `stage3_1_ram_gating` **6** (+1 ignored),
>   `benchmark_stage_2_7` **2** (+1 ignored), plus all other suites). `cargo
>   check --all-targets` clean; `cargo clippy --all-targets` introduces **no
>   new warnings** on changed code (remaining warnings are pre-existing in
>   untouched files).
>
> > **Design notes / verifications (2026-09-17):** (1) a retry is not a batch
> > failure — a job that succeeds on retry is counted exactly once as
> > completed and 0 failures, and no double counting of `failed_jobs` or
> > `processed_duration_secs` occurs across the processor/scheduler boundary
> > (verified by tracing each failure site). (2) Retry semantics deliberately
> > wait for the whole in-flight wave: the join loop must drain every admitted
> > task before the outer loop re-admits a retry, which satisfies "let all
> > currently in-flight jobs continue running and finish normally; once
> > capacity allows under the new, reduced ceiling, retry". (3) The ceiling
> > is never raised within a batch run and never drops below 1; `total_capacity`
> > stays the configured baseline so Stage 2.1 completions/tests remain exact.
>   (4) The classification list is intentionally a small stable set and is easy
>   to extend later.

---

**Phase 3 exit gate:** The scheduler behaves safely under RAM pressure, disk pressure, and mixed subtitle/normal workloads, distinguishes between "don't bother retrying" and "worth a reduced-capacity retry" failures, and never interrupts already-running work to react to an unrelated job's failure.

---

# ARCHITECTURE REWORK — `architecture_fix.md` (applied 2026-09-23)

An architecture follow-up (`architecture_fix.md`) audited the process-concurrency design end to end and required eight staged changes, all implemented and validated. It **did not** reshape the roadmap's capacity-based scheduler model — it removed a production footgun, made fail-safe behavior explicit, and refined queue ordering.

| # | architecture_fix stage | Change applied |
| - | ---------------------- | -------------- |
| 1 | Remove production `ffmpeg_threads_per_job = 1` | `ConcurrencyPlan.ffmpeg_threads_per_job` deleted (`concurrency.rs`); `batch_processor.rs` no longer threads a per-job count through `process_batch_job`/`ResolvedJob`; both production `ResolvedJob` constructions set `threads_per_job: None`. |
| 2 | Never force `-threads` in production (FFmpeg AUTO) | `ffmpeg_args_builder.rs` keeps the `Option<usize>` capability but production batch jobs never pass it — no `-threads` flag is emitted for batch renders. |
| 3 | Scheduler owns process concurrency | Unchanged by design — `total_capacity` admission is the sole concurrency control; the audit confirmed this and kept it. |
| 4 | Conservative when uncertain | `calculate_safe_concurrency` pins `logical_cpu_threads == 0` → capacity 1 and `available_memory_bytes == 0` → capacity 1 (never yield baseline parallelism from an unknown read); new tests: `zero_cpu_threads_resolves_conservatively`, `zero_available_ram_resolves_conservatively`, `missing_ram_and_cpu_never_yield_parallel`. |
| 5 | Adaptive admission | Already satisfied by the Stage 2.1 `CapacityGate` + Stage 3.4 `admission_ceiling`; the audit required no change. |
| 6 | Cost estimation from known config | `scheduler::estimate_job_cost(&BatchJob) -> f64` (baseline 1.0 at 1080p/H.264/medium; pixel-area factor `target_w*target_h / (1920*1080)` clamped `[0.1, 8.0]`, `None` = 1.0; WebM ×1.5; subtitle pipeline ×1.6; background ×1.15; logo ×1.1; text overlay ×1.15; any overlays ×1.1; transform ×1.05; color filter ×1.05; `remove_audio` ×0.92; speed-preset map ultrafast→veryslow 0.6→1.8, unknown = 1.0). No input-size/duration probing, no per-admission IO. |
| 7 | Lower-cost ordering | `pop_lowest_cost_job(&mut BatchState)` replaces strict `pop_front()` in admission: stable linear scan over the `VecDeque`, strict `<`, earliest-index FIFO tie-break, `VecDeque::remove(best_index)`. Remaining `pop_front()` uses are the disk-block / backlog drain loops, intentionally unchanged. |
| 8 | `in_flight_cost <= total_capacity` | Revalidated; the ceiling accounting (`admission_ceiling` never > `total_capacity`, floor 1) is intact, and all capacity tests remain green. |

**Validation:** `cargo test` — **184 passed, 0 failed** (lib 184, no integration targets — the one-shot integration suite was removed; see below). `cargo check --all-targets` clean; `cargo clippy --all-targets` adds no new warnings (remaining warnings are pre-existing in untouched files). New/updated tests include the three Stage-4 fail-safe tests, `plan_carries_no_per_job_ffmpeg_thread_hint`, six `estimate_job_cost` unit tests, `scheduler_admits_lowest_estimate_first` (capacity-2 admission order `[cheap, cheap, subtitle]`), and `equal_estimate_jobs_keep_fifo_order` — plus reworked older tests that no longer carry thread-hint asserts.

**Cleanup done as part of the fix:** the integration suite was one-shot (scenarios superseded by in-suite unit tests); with `tests/` empty, `build.rs`'s `cargo:rustc-link-arg-tests` directive broke every build (invalid-instruction error), so both the directive and the empty `tests/` tree were removed. The Stage 2.7 benchmark record file was also removed (conclusions preserved above). The subtitle-cost and failure-classification evidence in Stages 3.2/3.4 remains valid and unchanged by this rework.

---

## Phase 4 — Hardware Acceleration (deferred, optional follow-up)

**Do not start Phase 4 until Phases 1–3 are stable in real-world use for a reasonable period, and Stage 2.7's benchmarks have established a solid, trusted CPU-only baseline.** A validated CPU-only baseline is what lets you actually tell, later, whether GPU support is providing a real benefit — jumping to GPU detection before that baseline exists would make it impossible to attribute performance changes correctly. This phase is explicitly a separate, later feature — not part of the initial parallel-processing release. Included here for completeness and so the Phase 1–3 abstractions are built with this in mind, not so it gets implemented immediately.

### Stage 4.1 — FFmpeg hardware capability discovery

**Status:** [ ]
Query the bundled FFmpeg sidecar binary directly via `ffmpeg -hwaccels` and `ffmpeg -encoders` at runtime (do not infer from GPU name/vendor strings). Look for relevant encoder entries (e.g. `h264_nvenc`, `h264_qsv`, `h264_amf`). Availability of an encoder string does not guarantee it will actually initialize — a capability check alone is not sufficient (see Stage 4.2).

### Stage 4.2 — Encoder abstraction + real initialization verification

**Status:** [ ]
Extend `ResourceBudget`/`ConcurrencyPlan` with optional `gpu` / `hardware_encoder` fields (the types from Stage 1.2 were deliberately left extensible for this). Before relying on a hardware encoder, verify it can actually initialize and that the current filter pipeline (blur, overlays, subtitles, transforms) is compatible — naively swapping `libx264` for `h264_nvenc` without accounting for GPU↔CPU memory copies in the filter graph can erase the performance benefit entirely.

### Stage 4.3 — GPU-aware concurrency

**Status:** [ ]
Once 4.1/4.2 are solid, extend the concurrency planner to account for encoder session capacity and VRAM, not just "GPU present." Session/resource-sharing behavior varies significantly by GPU vendor and class — do not assume linear scaling with detected GPU count. Benchmark this the same way Stage 2.7 benchmarked CPU concurrency, against the established CPU-only baseline.

---

## Phase 5 — Adaptive Runtime Scheduling (optional, future)

**Status:** [ ] Explicitly optional; not required for a complete, shippable feature.

Add runtime resource monitoring that can shrink the admission ceiling of an already-running batch (beyond the batch-start-only planning done in Phase 1/3, and beyond the reactive, failure-triggered reduction in Stage 3.4) if resource pressure is detected mid-batch through direct monitoring. Treat this as a nice-to-have refinement after real-world usage data from Phases 1–3 shows it's actually needed, rather than building it speculatively.

---

# TESTING & VALIDATION MATRIX

Run relevant rows after the phase noted. This matrix is referenced by letter from the stages above.

| ID  | Test                                              | Run after                | What to verify                                                                                                                                                                            |
| --- | ------------------------------------------------- | ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A   | Single video                                      | Phase 0 (baseline)       | Output identical to pre-refactor; progress/cancel/skip/error paths unaffected                                                                                                             |
| B   | Two videos concurrently                           | Phase 2                  | Correct output for both; independent progress; no cross-talk; no lock collision                                                                                                           |
| C   | 10–20 videos                                      | Phase 2                  | Capacity stays capped; queue drains correctly; no job skipped/duplicated                                                                                                                  |
| D   | Same source, multiple targets (9:16, 1:1, 16:9)   | Phase 2                  | These now run _concurrently_ (validates Stage 0.1's revised, target-scoped lock) with no false collision, while a true duplicate (same input + same target queued twice) is still blocked |
| E   | CPU tiers: 4/8/16/32 core machines (or simulated) | Phase 1 & 2              | Concurrency plan matches the tier table; hard cap of 4 respected                                                                                                                          |
| F   | Low RAM (4GB/8GB)                                 | Phase 1 & 3              | Planner forces sequential or capped concurrency correctly                                                                                                                                 |
| G   | Long videos (30/60/120 min)                       | Phase 2                  | Progress/ETA doesn't drift with concurrent jobs of different lengths                                                                                                                      |
| H   | Mixed workload (720p/1080p/1080p+subs/4K/720p)    | Phase 3                  | Scheduler behaves sensibly; subtitle cost weighting respected; in-flight cost never exceeds total_capacity                                                                                |
| I   | One deliberate FFmpeg failure in a batch          | Phase 2                  | Other jobs continue; correct `Completed/Failed` tally; capacity released correctly                                                                                                        |
| J   | Multiple simultaneous failures                    | Phase 2                  | No deadlock; capacity has no leaks; remaining queue keeps processing                                                                                                                      |
| K   | Cancel with 2–4 active FFmpeg jobs                | Phase 2 (Stage 2.5)      | All child processes confirmed terminated before UI reports `Cancelled`                                                                                                                    |
| L   | Simulated low/no disk space                       | Phase 3                  | Clear error, no corrupt output, temp files cleaned, no infinite retry                                                                                                                     |
| M   | Resource-related failure mid-batch                | Phase 3 (Stage 3.4)      | Already-running jobs unaffected and finish normally; new-job admission ceiling drops; failed job retried under reduced capacity                                                           |
| N   | Benchmark sweep (720p/1080p/4K × capacity 1–4)    | Phase 2 (Stage 2.7)      | Documented wall-clock comparison; tier table defaults confirmed or revised                                                                                                                |
| O   | Hardware encoder limits (session/VRAM exhaustion) | Phase 4 (if implemented) | Graceful downgrade, no crash                                                                                                                                                              |

---

# FILES TOUCHED — SUMMARY

| File                                                                                   |               Phase 0                |         Phase 1         |                  Phase 2                  |                 Phase 3                  |      Phase 4      |
| -------------------------------------------------------------------------------------- | :----------------------------------: | :---------------------: | :---------------------------------------: | :--------------------------------------: | :---------------: |
| `video/lock.rs`                                                                        | ✅ required (target-scoped identity) |                         |                                           |                                          |                   |
| `video/batch_processor.rs`                                                             |        ✅ required (extract)         |                         |      ✅ required (scheduler wiring)       |                                          |                   |
| `video/ffmpeg_args_builder.rs`                                                         | ✅ required (thread flag capability) |                         |      ✅ required (wire real values)       |                                          | ✅ (encoder args) |
| `video/queue.rs`                                                                       |     ✅ required (progress math)      |                         |        ✅ (event emission review)         |           ✅ (admission logic)           |                   |
| `video/types.rs`                                                                       |                                      | ✅ required (new types) |                                           |                                          | ✅ (extend types) |
| `video/concurrency.rs` (new)                                                           |                                      | ✅ required (new file)  |                                           |               ✅ (extend)                |    ✅ (extend)    |
| `video/scheduler.rs` (new, optional split)                                             |                                      |                         | ✅ (new file — capacity-based dispatcher) | ✅ (extend for cost + ceiling reduction) |                   |
| Frontend batch UI                                                                      |                                      |                         |        ✅ (verification pass only)        |                                          |                   |
| `convert.rs`, `filter_builder.rs`, `render_layout.rs`, `preset_adapter.rs`, `probe.rs` |       **untouched throughout**       |                         |                                           |                                          |                   |

---

# RISK REGISTER (cross-phase)

| Risk                                                                              | Mitigated by                                                                        |
| --------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| Output file lock collisions across concurrent jobs                                | Stage 0.1                                                                           |
| Lock over-serializing legitimate same-source/different-target concurrency         | Stage 0.1 (revised, target-scoped identity)                                         |
| Silent behavior drift from refactor before concurrency is even added              | Stage 0.2 + 0.5 baseline gate                                                       |
| Mistaking `-threads N` for an exact CPU accounting mechanism                      | Stage 0.3 caveat + admission control as primary guard + Stage 2.7 benchmarking      |
| Production forcing a per-job `-threads` hint, overriding FFmpeg AUTO and oversubscribing cores | architecture_fix Stages 1/2/6 — hint removed; batch renders run AUTO; capacity is process-level |
| CPU oversubscription making parallel slower than sequential                       | Stage 1.3 conservative tiers + Stage 2.7 real benchmarking                          |
| Wrong/misleading progress and ETA with multiple active jobs                       | Stage 0.4                                                                           |
| Scheduler plumbing bugs mistaken for "concurrency is broken"                      | Stage 2.2 parity check (capacity=1 first)                                           |
| One failing job taking down the whole batch or leaking capacity                   | Stage 2.1 (per-task error containment) + Stage 2.6 verification                     |
| Premature "Cancelled" state while FFmpeg children still shutting down             | Stage 2.5                                                                           |
| Shipping unvalidated concurrency defaults                                         | Stage 2.7 benchmarking, feeding back into Stage 1.3                                 |
| Subtitle/Whisper jobs silently multiplying concurrent process count               | Stage 3.2 (now a natural extension of the Stage 2.1 capacity model)                 |
| Disk exhaustion from simultaneous temp output files                               | Stage 3.3                                                                           |
| Wasting time retrying unfixable (deterministic) FFmpeg errors                     | Stage 3.4 classification                                                            |
| Killing/interrupting healthy in-flight jobs to react to a different job's failure | Stage 3.4 (revised: reduce future admission ceiling only, never touch running jobs) |
| Jumping to GPU encoding before CPU parallelism is proven stable and benchmarked   | Phase 4 explicitly gated behind Phase 1–3 stability _and_ Stage 2.7's baseline      |

---

# SUGGESTED PROMPT-SPLITTING GUIDE FOR OPENCODE

Turn each stage above into one OpenCode prompt, in this order. Suggested prompt skeleton per stage:

```
Context: AspectShift-HtoV, implementing roadmap.md Stage <X.Y> only.
Read: roadmap.md Stage <X.Y> section in full before writing any code.
Scope: <paste the "Required change" bullet list for that stage>
Do NOT touch: <paste "leave alone" files if listed for that stage>
Validation before marking done: <paste that stage's "Validation" section>
Report back: what changed, why, and confirmation each validation step passed.
```

Do not let OpenCode combine two stages into one prompt/session, even if it offers to — the whole point of the phase/stage split is that each one has an isolated, checkable blast radius. Update the `[ ]` → `[x]` status markers in this file as each stage is confirmed complete before generating the next stage's prompt.
