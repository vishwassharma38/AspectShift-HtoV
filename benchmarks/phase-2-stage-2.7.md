# Stage 2.7 — Benchmark representative workloads before finalizing defaults

**Status:** NOT COMPLETE — Machine Class A (low/mid-range) measured; Machine Class B
(higher-end) **blocked: no higher-end machine is available in the current
environment**. Stage 2.7 remains **in progress** in `roadmap.md` (`[~]`).
All data below is real, measured wall-clock data captured on the machine
described in §2. No measurement has been estimated, fabricated, or omitted.
Second-pass audit (2026-09-17): implementation verified directly; benchmark
override isolation proven (see §9); invalid-override handling tested (see §9.1);
baseline scope limitation documented (see §10).

## 1. Date

2026-09-17 (all runs executed in a single sweep on one day and machine).

## 2. Machine information

### Machine Class A — low/mid-range desktop (MEASURED)

| Field | Value |
| ----- | ----- |
| Machine class | Low/mid-range (representative mainstream desktop) |
| CPU | AMD Ryzen 5 3500 |
| Cores / threads | 6 physical cores / 6 logical threads (no SMT) |
| RAM | 15.9 GB total; **4.2 GiB available** at batch start during the sweep (as detected by `detect_system_resources()`) |
| GPU | NVIDIA GeForce GTX 1660 SUPER (6 GB) — not used by this CPU-only FFmpeg path; recorded for completeness |
| OS | Windows 10 Pro, version 10.0.19045 |
| FFmpeg | `ffmpeg 8.1-essentials_build-www.gyan.dev` (gcc 15.2.0), libavcodec 62, libx264 enabled |
| App / build | This repository's `src-tauri` crate (lib `aspectshift_htov_lib`), `cargo test` debug profile, harness `tests/benchmark_stage_2_7.rs` |

**Why this machine is the low/mid-range class:** a 6-thread mainstream desktop
CPU with no SMT sits squarely in the Stage 1.3 "5–8 logical threads → 2" tier,
and its ~4 GiB of *available* RAM lands between the 4 GiB and 8 GiB RAM-gate
boundaries, so the natural plan resolves to `total_capacity = 2`. It is not
labeled low/mid-range merely for convenience; it is the affordable mainstream
class this app targets, and both the CPU tier and the RAM gate exercise real
boundary behavior on it.

### Machine Class B — higher-end machine (BLOCKED — no hardware available)

No Class B measurements were taken and none are fabricated. The current
environment was probed specifically for the Class B requirement
(2026-09-17, second-pass audit):

- The only machine physically/actually available is the **Machine Class A
  hardware itself** (AMD Ryzen 5 3500, 6 C / 6 T, 15.9 GB RAM, checked via
  `Win32_Processor` / `Win32_ComputerSystem` / `Win32_OperatingSystem`).
- No WSL distribution is installed (`wsl --status` empty).
- No remote benchmark host is configured: `git remote` points only at the
  GitHub repository; no GitHub Actions workflow (or other CI config) exists
  that provisions a higher-end benchmark runner; the only workflow
  (`.github/workflows/release.yml`) is a release build, not a benchmark host.

Because Class B must be **real, measured performance evidence** on genuinely
higher-end representative hardware (16+ logical threads / ≥ 16 GiB RAM, or
any machine clearly above the Class-A CPU tier), and no such machine is
reachable from here, the Class B requirement is an explicit **blocker**.
Stage 2.7 stays `[~]` in `roadmap.md` until a higher-end machine becomes
available. See §8 for the exact procedure and report template to fill then.

## 3. Workload definition

All clips are synthetic MP4 media generated with the bundled FFmpeg sidecar
via `tests/benchmark_stage_2_7.rs::make_clip`. Generation time is excluded from
every measurement (§4). Exact generation pattern per clip:

```text
ffmpeg -y -f lavfi -i "testsrc2=duration=<D>:size=<W>x<H>:rate=30"
       -f lavfi -i "sine=frequency=440:duration=<D>"
       -c:v libx264 -preset veryfast -c:a aac -shortest -movflags +faststart out.mp4
```

`testsrc2` was chosen over `testsrc` because it is a moving pattern with
higher-frequency detail, giving normal motion/content at a representative frame
rate (30 fps) in representative codecs (H.264 video + AAC audio). Audio is a
440 Hz sine so the real `-map 0:a?` / AAC re-encode path is exercised through
every conversion. All inputs are generated at their native resolutions — no
resolution is derived by scaling another clip.

| Workload | Res. used for conversion | Clips | Per-clip duration | Container / codecs | fps | Target output (9:16, height-limited to 1920) |
| -------- | ------------------------ | ----: | ----------------: | ------------------ | --: | ------------------------------------------- |
| A — 720p | 1280×720 native | 4 | 12 s | MP4 / H.264 + AAC | 30 | ≈406×720 |
| B — 1080p | 1920×1080 native | 4 | 12 s | MP4 / H.264 + AAC | 30 | ≈608×1080 |
| C — 4K | 3840×2160 native | 2 | 10 s | MP4 / H.264 + AAC | 30 | ≈1080×1920 |

**Output / processing configuration (unchanged production defaults, identical
across every run):** `EncodingProfile::standard()` (CRF 23, speed `medium`,
AAC 128 kbps), target aspect `9:16`, default effects (no blur/logo/subtitles),
subfolders off. This is the exact `start_batch` → Stage 2.1 scheduler →
`process_batch_job` → `render_single` → FFmpeg path real users take; only
`total_capacity` was parameterized via the benchmark-only hook
`ASPECTSHIFT_BENCH_TOTAL_CAPACITY`, which derives the per-job `-threads` hint
with the same Stage 1.3 formula the normal planner uses.

## 4. Measurement methodology

- **Timing starts** immediately before `start_batch(...).await` (after the app
  instance and input list are built, which are setup and excluded).
- **Timing ends** the moment `get_batch_status` reports a terminal state
  (`Completed`). The whole user-facing batch is inside the window: parallel
  probe + thumbnail preparation, FFmpeg render, temp-output finalize.
- **Excluded:** clip generation, app construction, harness plumbing, cleanup.
- **Only one variable changes between runs:** `total_capacity` (1, 2, 3, 4).
  Inputs, count, resolution, duration, output settings, FFmpeg args, preset,
  aspect, effects, machine, and power state are constant. The scheduler itself
  is never altered.
- **Thread hint per forced capacity** (Stage 1.3 formula on 6 threads:
  `max(1, floor((6 − 4) / capacity))`): capacity 1 → `-threads 2`, capacities
  2/3/4 → `-threads 1`. Following the thread hint through the real FFmpeg args
  builder is part of what is being validated.
- **Runs:** 3 per (workload × capacity) cell (12 cells × 3 = 36 measured
  batches). All individual times are recorded (§5); the summary value is the
  arithmetic mean. No run was cherry-picked or discarded.
- **Environment:** machine idle (no competing foreground load) during the
  sweep; `--test-threads=1` ensures the process-global capacity env var cannot
  race.
- **Repeatability:** the sweep is a `#[ignore]`d integration test:
  `cargo test --test benchmark_stage_2_7 -- --ignored --test-threads=1 --nocapture`
  (`BENCH_RUNS` and `BENCH_CAPACITIES` control repetitions/capacity subset).
  Every cell self-verifies: batch `Completed`, tally = input count, per-job
  progress 100 %, every output validated as a real video via `ffprobe`, and max
  simultaneous `Processing` jobs == `min(capacity, jobs)` (never above the
  forced capacity) — so corrupt/missing outputs or an override the scheduler
  ignored would fail the cell.

## 5. Raw results

All values are total batch wall-clock times in seconds, mean of the recorded
runs. Lower is better.

### Workload A — 720p (4 clips × ~12 s)

| Capacity | Run 1 | Run 2 | Run 3 | Mean |
| -------: | ----: | ----: | ----: | ----: |
|        1 |  5.98 |  6.05 |  6.44 |  6.16 |
|        2 |  6.14 |  6.21 |  6.11 |  6.15 |
|        3 |  6.19 |  5.96 |  6.49 |  6.21 |
|        4 |  4.44 |  4.61 |  4.13 |  4.39 |

### Workload B — 1080p (4 clips × ~12 s)

| Capacity | Run 1 | Run 2 | Run 3 | Mean |
| -------: | ----: | ----: | ----: | ----: |
|        1 |  9.56 |  9.71 |  9.36 |  9.54 |
|        2 |  9.86 | 10.15 | 10.02 | 10.01 |
|        3 | 10.02 |  9.81 | 10.06 |  9.96 |
|        4 |  6.27 |  6.17 |  6.37 |  6.27 |

### Workload C — 4K (2 clips × ~10 s)

| Capacity | Run 1 | Run 2 | Run 3 | Mean |
| -------: | ----: | ----: | ----: | ----: |
|        1 | 12.10 | 11.87 | 12.09 | 12.02 |
|        2 | 12.85 | 13.04 | 13.19 | 13.03 |
|        3 | 13.06 | 13.05 | 12.56 | 12.89 |
|        4 | 12.85 | 13.51 | 13.75 | 13.37 |

Within-cell variance is small (±0.1–0.5 s) apart from the expected 720p
cap-1/cap-3 samples (±0.4–0.5 s); no unusually large variance was observed and
no outlier was removed. Observed concurrency matched the forced capacity in
every cell (4-job workloads: exactly 1/2/3/4 simultaneous `Processing` jobs;
4K: a max of 2 even at capacities 3–4, because only two jobs exist).

## 6. Interpretation

### Which capacity won per workload

- **720p:** capacity 4 (4.39 s). Capacities 1–3 are statistically identical
  (6.15–6.21 s). Capacity 4 is **1.40× faster** than capacity 1 and **1.40×
  faster** than capacity 2.
- **1080p:** capacity 4 (6.27 s). Capacities 1–3 again flat (9.54–10.01 s).
  Capacity 4 is **1.52× faster** than capacity 1 and **1.60× faster** than
  capacity 2; capacities 2–3 were ~5 % *slower* than sequential.
- **4K:** capacity 1 (12.02 s) — sequential wins. Capacities 2, 3, 4 all run
  the same two jobs with the per-job thread hint halved to 1 and were ~8–11 %
  slower (12.89–13.37 s); capacity 4 is ~3 % slower than capacity 2.

### Capacity 2 vs capacity 4 (explicit, per workload)

| Workload | 2 → 4 change | Conclusion |
| -------- | ----------- | ---------- |
| 720p     | 6.15 s → 4.39 s (−29 %) | 4 clearly beats 2 |
| 1080p    | 10.01 s → 6.27 s (−37 %) | 4 clearly beats 2 |
| 4K       | 13.03 s → 13.37 s (+3 %) | within noise; effectively equal, both beaten by 1 |

### Diminishing returns / contention

On this 6-thread machine, capacities 2 and 3 deliver **no wall-clock benefit**
over sequential for multi-video batches: 2–3 concurrent 1-thread encodes only
occupy ~2–3 of 6 threads and gain nothing over sequential 2-thread encodes
(which use the same aggregate thread budget without cross-process overhead).
The first meaningful win appears only at capacity 4, where four concurrent
1-thread jobs finally occupy 4 of 6 threads and cut total batch time by
~29–34 % for the 4-video workloads. Mild contention *was* observed — but only
in the small-job-count 4K cell, where the forced `-threads 1` loses more than
row parallelism gains (see §6/§7 thread-hint finding).

Caution: these 4-video batches are the exact pattern the roadmap calls out;
the finding that "4 is clearly better than 2 for 4-video batches on this CPU
tier" contradicts the single-machine guess that 2 might generally be the better
default for 5–8-thread machines — but it is real, measured data, and must be
treated as provisional until the higher-end machine is also measured.

## 7. Stage 1.3 decision

- **Defaults changed? No.** `calculate_safe_concurrency` (CPU tier table, RAM
  gate, hard cap 4, thread formula) and the roadmap tier table are **unchanged**
  and the code's "provisional, pending Stage 2.7" TODO stands, now backed by
  Class-A evidence only.
- **Why unchanged:** the defaults are conservative by design, and the decision
  between "keep tier = 2" vs "raise small machines toward 4" cannot be finalized
  on a single, RAM-gated machine class. Note the decisive interaction on this
  machine: it is already RAM-gated to 2 (< 8 GiB available), so its natural
  plan is 2 regardless of the CPU tier — and the measured data shows 2 gives no
  benefit over sequential while 4 (which the RAM gate deliberately blocks)
  gives the largest measured win. Churning the CPU tier table without a
  second machine class — and without Stage 3.1 validating whether the RAM gate
  thresholds are protecting real concurrent FFmpeg/Whisper — would be
  overfitting to one sample.
- **Recommended default (`total_capacity`):** remain the tier-derived value.
  On Class A it is 2 (RAM-gated). This is a *safe* default (sequentially
  equivalent, never slower in practice here). The open question — "should
  honest 4-video batches on 5–8-thread machines target 4?" — is explicitly
  deferred to the two-machine comparison; the measured evidence supports
  revisiting it.
- **Hard ceiling of 4:** remains. Capacity 4 never oversubscribed the machine
  (≤ 4 concurrent jobs, ≤ 4 of 6 threads, within headroom) and produced valid,
  complete outputs every run — the ceiling is not the problem; whether the
  planner should aim for it is.
- **`ffmpeg_threads_per_job` finding:** the hint is doing real, correct work
  and is **not** overly aggressive. It is the decisive variable in the 4K
  2-job cell — sequential with `-threads 2` beat all parallel cells with
  `-threads 1` — i.e. it meaningfully controls per-encode throughput for large
  frames. On 6 threads the formula never oversubscribes (max 4 × 1 = 4 active
  encode threads, leaving the 4-thread design headroom the app reserves for
  OS/UI/Whisper). The combination of admission control + thread hint behaved as
  intended in every cell (useful concurrency at cap 4, no slowdown from
  over-allocation at caps 2–3, hint respected at cap 1). No change to the
  formula is recommended before Class B is measured (16+ thread machines will
  exercise higher per-job hints, e.g. 6/job at cap 2, which cannot be validated
  here).
- **Scope note:** the default path encodes to height-limited (≤1920) portrait
  outputs with no background effects, so the encode component is modest; the
  ratios above reflect that production default. Effects-enabled conversions
  (blur/white background) or taller outputs would shift the balance toward
  encode dominance and are a documented future extension, not something this
  sweep claims to cover.

## 8. Machine Class B — procedure and report template (BLOCKED)

The two-machine requirement is **not** satisfied (no higher-end machine
available in this environment; see §2); Stage 2.7 is therefore NOT marked
complete. Run the procedure below on an actual higher-end machine (e.g.
a 16+ logical-thread desktop with ≥ 16 GiB RAM; anything clearly above the
Class-A CPU tier) and append a filled Class B section here before finalizing
Stage 1.3.

Procedure:

1. Check out this repository at the same commit. Confirm the FFmpeg sidecars
   exist (`src-tauri/bin/*.exe`).
2. Record hardware/software per the template below (CPU, cores/threads, RAM,
   GPU, OS, FFmpeg version; capture `cargo test --test benchmark_stage_2_7 -- --nocapture
   --test-threads=1` non-ignored pre-check output which prints the detected
   profile and natural plan).
3. Ensure the machine is otherwise idle; set the power plan to the same
   (balanced/performance) state used for Class A.
4. Run the sweep:
   ```text
   cargo test --test benchmark_stage_2_7 -- --ignored --test-threads=1 --nocapture
   ```
   (default: 1 run/cell; use `BENCH_RUNS=3` to match Class A repetitions).
5. Copy the printed `| workload | capacity | run | wall_clock_s | max_concurrency |`
   rows into the template below; compute means the same way (arithmetic mean of
   the recorded runs).
6. Update §6/§7 of this report: repeat the "capacity 2 vs 4" comparison per
   workload, re-run the Stage 1.3 decision with both machine classes in hand,
   and then (and only then) mark Stage 2.7 `[x]` in `roadmap.md`.

Template (fill and insert as "Machine Class B — higher-end machine (MEASURED)"):

```text
## 2b. Machine Class B — higher-end machine (MEASURED)

| Field | Value |
| ----- | ----- |
| Machine class | Higher-end |
| CPU | <model> |
| Cores / threads | <P>C / <T>T |
| RAM | <GB> total; <GiB> available at batch start |
| GPU | <model — CPU-only path, for completeness> |
| OS | <os/version> |
| FFmpeg | <version/config> |
| App / build | same commit as Class A |

### Raw results
(repeat the three workload tables from §5 verbatim, same format)

### Interpretation
- best capacity per workload;
- capacity 2 vs 4 relationship (faster / equal / slower) per workload;
- diminishing returns / contention observations;
- per-job thread hint behavior (16+ thread machines will exercise higher -threads values)
```

## 9. Tooling and reproducibility

- Harness: `src-tauri/tests/benchmark_stage_2_7.rs`.
  - `forced_capacities_are_accepted_and_respected_by_the_real_path` (runs with
    the normal suite) verifies capacities 1–4 are genuinely accepted and
    exercised by the real batch path.
  - `invalid_benchmark_override_values_fall_back_safely` (runs with the normal
    suite) proves invalid override values (0, 5, negative, non-numeric, empty)
    can never admit more than the hard safety ceiling (§9.1).
  - `benchmark_representative_workloads_across_capacities` (`#[ignore]`) is the
    sweep above.
- Implementation hook: `ASPECTSHIFT_BENCH_TOTAL_CAPACITY` read once per
  `start_batch` in `src-tauri/src/video/batch_processor.rs`, routed to
  `calculate_safe_concurrency_with_capacity` in `src-tauri/src/video/concurrency.rs`
  (clamped 1..=4, mode + `-threads` derived by the same formula as the normal
  planner; the scheduler itself is untouched). Unset, behaviour is byte-for-
  byte identical to a normal batch.
- No benchmark artifacts remain in the repository. Sweep media and outputs are
  written to the OS temp dir and were deleted after results were captured;
  `.gitignore` requires no additions.

## 9.1 Benchmark override isolation (audit, 2026-09-17)

Verified directly against the code paths, not assumed.

- **Normal application behavior (override unset):** `start_batch` reads the env
  var once and, when absent, calls `calculate_safe_concurrency(state)` — the
  Stage 1.3 planner (CPU tier + RAM gate + hard cap 4) is authoritative on
  every normal run. No benchmark-only code is reachable without the variable.
- **Benchmark-only access:** the only readers of the env var are
  `batch_processor.rs` (production call site, gated on the variable being set)
  and the benchmark tests themselves. No other production module reads it.
- **No frontend/user configuration:** the variable is not exposed by the UI,
  the settings schema, or any persisted configuration. `grep` across the
  frontend finds no concurrency/capacity setting; the only `ASPECTSHIFT*`
  frontend strings are the license-key placeholders.
- **Invalid values (tested):** values outside 1..=4 are safe by construction
  and by test:
  - parseable `0` → `calculate_safe_concurrency_with_capacity` clamps to 1
    (strictly sequential);
  - parseable `5` (and anything above) → clamps to the hard ceiling 4;
  - negative, non-numeric, and empty values fail `usize` parsing → the
    override is ignored and the standard Stage 1.3 plan is used.
  - `clamp(1, MAX_TOTAL_CAPACITY)` means no path — including direct calls to
    `calculate_safe_concurrency_with_capacity` — can produce a plan with
    `total_capacity > 4`, so the scheduler's safety invariant
    (`in_flight_cost <= total_capacity <= 4`) is unbreakable by the override.
- **No persistence/leak:** the override is a process-environment variable only.
  Nothing persists it as an application setting, writes it into release
  configuration, or propagates it across runs; the RAII `CapacityOverride`
  guard in the harness restores/removes it after every benchmark cell.
- **Planner remains authoritative:** with the override absent, normal planning
  is identical to pre-benchmark behavior (verified by the full regression and
  concurrency suites, all run with the variable unset).

## 10. Limitations of the baseline Stage 2.7 sweep

Baseline Stage 2.7 benchmarking covers **controlled standard-profile
conversions** (no background effects, no overlays, no subtitles, no heavier
filter graphs). The audit explicitly confirmed this scope:

> Baseline Stage 2.7 benchmarking covers controlled standard-profile
> conversions. Effects-enabled, subtitle-heavy, and other workload-specific
> filter graphs remain outside the baseline benchmark scope and are deferred
> to workload-specific validation in Phase 3.

This limitation is intentional and keeps Stage 2.7 a bounded exercise: it does
not claim coverage of blur / white-background processing / overlays / subtitles
/ heavier filter-graph workloads, and it must not be silently forgotten when
the two-machine comparison is completed.