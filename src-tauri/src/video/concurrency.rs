//! System resource detection (roadmap Stage 1.1), concurrency/capacity types
//! (roadmap Stage 1.2), the Stage 1.3 conservative planner, the Stage 3.1
//! batch-start planning entry point, and the Stage 3.3 disk-space admission
//! gate.
//!
//! Resource detection is the read-only, side-effect-free foundation for the
//! capacity-based scheduler. Together with the concurrency plan types and the
//! planner, this module describes what hardware capacity exists and how a safe
//! plan is derived from a *live* observation. Stage 3.1 guarantees that every
//! batch start performs its own fresh `detect_system_resources()` read via
//! [`plan_for_batch_start`] / [`resolve_batch_start_plan`] — no resource
//! snapshot, capacity value, or plan is ever cached or reused across batches.
//!
//! Stage 3.3 adds the disk-space admission gate: before the scheduler acquires
//! capacity for a new job it asks [`DiskAdmissionGate`] how many bytes are free
//! on the filesystem hosting the job's target output. A live query against a
//! conservative safety margin decides whether the job may be admitted; the
//! query is repeated per job so a drop mid-batch surfaces on the very next
//! admission attempt.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, RefreshKind, System};

/// A snapshot of the machine's available compute and memory capacity.
///
/// `logical_cpu_threads` is the number of logical execution threads the OS
/// reports (physical cores times threads per core on typical hardware).
/// `available_memory_bytes` is the currently available system RAM, not the
/// total installed memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceProfile {
    pub logical_cpu_threads: usize,
    pub available_memory_bytes: u64,
}

/// Maps the relevant `sysinfo` values onto a `ResourceProfile`.
///
/// Kept separate so the extraction can be tested against a known system
/// snapshot without depending on a particular machine's resource state.
fn profile_from_system(system: &System) -> ResourceProfile {
    ResourceProfile {
        logical_cpu_threads: system.cpus().len(),
        available_memory_bytes: system.available_memory(),
    }
}

/// Detects the machine's logical CPU-thread count and available RAM.
///
/// Read-only and side-effect-free: no files, processes, environment, or
/// global state are touched. Only the CPU list and RAM information are
/// refreshed, so a single call per batch start is cheap.
///
/// On platforms `sysinfo` does not support, the returned values may be zero;
/// no fabricated values are ever substituted.
pub fn detect_system_resources() -> ResourceProfile {
    let system = System::new_with_specifics(
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing())
            .with_memory(MemoryRefreshKind::nothing().with_ram()),
    );
    profile_from_system(&system)
}

/// Whether a batch should be executed sequentially (one job at a time) or
/// with bounded parallelism.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Sequential,
    Parallel,
}

/// The machine resources a concurrency plan may assume are available.
///
/// `cpu_threads` is the logical CPU thread count the plan may budget across;
/// `available_memory_mb` is the available (not total) system RAM in
/// megabytes. GPU/encoder capacity fields are reserved for Phase 4 and are
/// intentionally not implemented here; the struct stays minimal and
/// forward-compatible instead of reacting to future requirements early.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceBudget {
    pub cpu_threads: usize,
    pub available_memory_mb: u64,
}

/// A plan describing how much concurrent work a batch may admit.
///
/// `total_capacity` is expressed in **cost units**, not literal worker
/// handles, OS threads, or FFmpeg process counts. For example, a plan with
/// `total_capacity = 4` can run 4 Normal-cost jobs (1 unit each), 2
/// Subtitle-cost jobs (2 units each), or any mix whose combined in-flight
/// cost does not exceed 4 units. The cost model is computed by
/// `scheduler::classify_job_cost` (Stage 3.2): normal jobs cost 1 unit,
/// subtitle/Whisper jobs cost 2 units.
///
/// The plan deliberately carries **no per-job FFmpeg thread count**
/// (architecture_fix Stage 1/2/6): FFmpeg owns its own internal threading and
/// the production invariant is FFmpeg `-threads AUTO`. AspectShift's only
/// concurrency responsibility is `total_capacity` — how many independent jobs
/// may be admitted at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConcurrencyPlan {
    pub mode: ExecutionMode,
    pub total_capacity: usize,
    pub resource_budget: ResourceBudget,
}

/// Absolute safety ceiling for any concurrency plan.
pub const MAX_TOTAL_CAPACITY: usize = 4;

/// Builds the derived fields of a plan (execution mode, resource budget) from
/// a `total_capacity`.
///
/// The caller is responsible for the capacity value having passed any policy
/// decisions (tier table, RAM gate, clamping); this function only derives the
/// consequences so that no plan can ever disagree with its own capacity.
fn derive_plan_fields(profile: &ResourceProfile, total_capacity: usize) -> ConcurrencyPlan {
    // --- Mode selection (derived from total_capacity) ---
    let mode = if total_capacity <= 1 {
        ExecutionMode::Sequential
    } else {
        ExecutionMode::Parallel
    };

    // --- Resource budget (for future use) ---
    let resource_budget = ResourceBudget {
        cpu_threads: profile.logical_cpu_threads,
        available_memory_mb: profile.available_memory_bytes / (1024 * 1024),
    };

    ConcurrencyPlan {
        mode,
        total_capacity,
        resource_budget,
    }
}

/// Computes a safe concurrency plan from a system resource profile.
///
/// Uses a fixed CPU tier table (not a dynamic formula) and a RAM gate to
/// determine `total_capacity`. No per-job FFmpeg thread hint is derived or
/// emitted (architecture_fix Stage 1/2/6): FFmpeg always runs with its own
/// internal AUTO threading in production.
///
/// The CPU tier table is provisional; Stage 2.7 benchmarking is expected to
/// validate or revise these defaults. The hard cap of 4 is a safety ceiling.
pub fn calculate_safe_concurrency(profile: &ResourceProfile) -> ConcurrencyPlan {
    // --- CPU tier table (conservative provisional defaults) ---
    // TODO: Stage 2.7 — benchmarking will validate or revise these values.
    // Benchmarking may demonstrate that a lower concurrency (potentially 2)
    // is better than allowing the ceiling of 4. Revisit after Stage 2.7.
    // Stage 2.7 evidence (2026-09-17, recorded in the roadmap; the one-shot
    // benchmark record file was removed in the architecture-fix cleanup).
    // Machine Class A (6 threads, RAM-gated to 2) is measured; Class B
    // (higher-end) is pending. On Class A, capacities 2–3 ≈ sequential and
    // capacity 4 was 29–37% faster for the 4-video workloads, so the table
    // stays unchanged provisionally until the two-machine comparison lands.
    //
    // Stage 4 fail-safe: a zero/nonsensical CPU observation is treated as
    // "we don't know the machine", which must resolve to *less* concurrency,
    // never more. `logical_cpu_threads == 0` therefore maps to Sequential (1).
    let cpu_tier_capacity = match profile.logical_cpu_threads {
        0 => 1,
        1..=4 => 1,
        5..=8 => 2,
        9..=16 => 2,
        17..=32 => 3,
        _ => 4,
    };

    // --- Hard cap enforcement (absolute maximum 4) ---
    // This remains true even for extremely high logical CPU counts.
    let cpu_tier_capacity = cpu_tier_capacity.min(MAX_TOTAL_CAPACITY);

    // --- RAM gate (applied after CPU tier) ---
    const FOUR_GB: u64 = 4 * 1024 * 1024 * 1024;
    const EIGHT_GB: u64 = 8 * 1024 * 1024 * 1024;

    let total_capacity = if profile.available_memory_bytes == 0 {
        // Stage 4 fail-safe: RAM detection failed/missing → assume the machine
        // cannot safely host concurrent encodes (Sequential).
        1
    } else if profile.available_memory_bytes < FOUR_GB {
        // Unconditional override: force sequential
        1
    } else if profile.available_memory_bytes < EIGHT_GB {
        // Cap at 2
        cpu_tier_capacity.min(2)
    } else {
        // Use CPU-tier value unmodified (subject to hard cap already applied)
        cpu_tier_capacity
    };

    derive_plan_fields(profile, total_capacity)
}

/// Resolves the concurrency plan that will govern the *next* batch.
///
/// This is the single batch-start planning entry point. It invokes `detect`
/// exactly once to obtain a **live** system-resource observation, then derives
/// a brand-new `ConcurrencyPlan` from it (Stage 1.3 CPU tier → RAM gate → hard
/// cap). Nothing here caches or reuses any previous observation or plan, so an
/// observation made for Batch A can never leak into Batch B.
pub fn resolve_batch_start_plan(detect: impl FnOnce() -> ResourceProfile) -> ConcurrencyPlan {
    let resources = detect();
    calculate_safe_concurrency(&resources)
}

/// Detects live system resources and resolves the plan governing the next
/// batch.
///
/// Stage 3.1 guarantee: this performs its own `detect_system_resources()` read
/// at *every* batch start (never a cached or stale snapshot) and derives a
/// fresh plan from it. A concise diagnostic is recorded at this boundary so the
/// `Batch → resource snapshot → plan` chain for consecutive batches (e.g.
/// `Batch A → snapshot A → plan A`, `Batch B → snapshot B → plan B`) is
/// observable without guesswork.
pub fn plan_for_batch_start() -> ConcurrencyPlan {
    let plan = resolve_batch_start_plan(detect_system_resources);
    let budget = plan.resource_budget;
    tracing::info!(
        "batch-start resource snapshot: logical_cpu_threads={}, available_ram_mb={} \
         -> total_capacity={} (mode={:?})",
        budget.cpu_threads,
        budget.available_memory_mb,
        plan.total_capacity,
        plan.mode,
    );
    plan
}

// -----------------------------------------------------------------------------
// Stage 3.3 — disk-space admission gate
// -----------------------------------------------------------------------------

/// Conservative free-space floor maintained before any new job is admitted.
///
/// Parallel encoding writes multiple temporary output files at once, and a
/// full disk on one of them silently truncates that job's render. The exact
/// byte requirement of a render is out of scope for v1 (the roadmap says so);
/// a simple "healthy margin" threshold is sufficient. A job may only be
/// admitted while the hosting filesystem reports **strictly more** free space
/// than this constant.
pub const DISK_SAFETY_MARGIN_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB

/// Abstraction over a filesystem free-space query.
///
/// The scheduler asks for free bytes on the filesystem that contains `path`
/// (the job's resolved target output). Implementations must be cheap, live
/// queries — free space is never cached, so a mid-batch drop is observed on
/// the next admission attempt. An `Err` means the value could not be
/// determined and is treated by the gate as an admission failure (fail closed).
pub trait DiskSpaceSource: Send + Sync + std::fmt::Debug {
    fn available_bytes(&self, path: &Path) -> io::Result<u64>;
}

/// The production [`DiskSpaceSource`], backed by `sysinfo`'s disk list.
///
/// The query refreshes the OS disk list and selects the volume whose mount
/// point is the longest path prefix of the probed path (matched
/// case-insensitively on Windows). Non-existent paths are walked up to their
/// nearest existing ancestor first, so a not-yet-created output file or
/// directory still resolves to the volume that will host it.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemDiskSpaceSource;

impl DiskSpaceSource for SystemDiskSpaceSource {
    fn available_bytes(&self, path: &Path) -> io::Result<u64> {
        let disks = Disks::new_with_refreshed_list();
        let probe = nearest_existing_ancestor_or_self(path);
        match matching_disk(&disks, probe) {
            Some(disk) => Ok(disk.available_space()),
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "no volume found containing {:?} (known mount points: {})",
                    probe,
                    mount_points_summary(&disks)
                ),
            )),
        }
    }
}

/// Walks up from `path` to the nearest ancestor that currently exists, so a
/// path whose parent directories (or file) do not exist yet can still be
/// matched to the volume that will eventually host it.
fn nearest_existing_ancestor_or_self(path: &Path) -> &Path {
    let mut current = path;
    while !current.exists() {
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    current
}

/// Returns the disk whose mount point is the longest prefix of `path`, or
/// `None` when no known mount contains it.
fn matching_disk<'a>(disks: &'a Disks, path: &Path) -> Option<&'a sysinfo::Disk> {
    let mounts: Vec<PathBuf> = disks
        .list()
        .iter()
        .map(|disk| disk.mount_point().to_path_buf())
        .collect();
    let chosen = choose_mount(&mounts, path)?;
    let index = mounts
        .iter()
        .position(|mount| mount.as_path() == chosen)
        .expect("chosen mount point came from the disk list");
    Some(&disks.list()[index])
}

/// Selects the mount point that is the longest prefix of `path` (the most
/// specific volume containing it), or `None` when no mount contains it.
fn choose_mount<'a>(mounts: &'a [PathBuf], path: &Path) -> Option<&'a Path> {
    let case_sensitive = !cfg!(windows);
    mounts
        .iter()
        .filter(|mount| path_on_mount(path, mount, case_sensitive))
        .max_by_key(|mount| mount.components().count())
        .map(|mount| mount.as_path())
}

/// Whether `path` sits under `mount` (component-wise prefix match, optionally
/// case-insensitive so `d:\...` still matches the `D:\` volume reported by the
/// OS on Windows).
fn path_on_mount(path: &Path, mount: &Path, case_sensitive: bool) -> bool {
    let mut path_components = path.components();
    for mount_component in mount.components() {
        match path_components.next() {
            Some(component)
                if component_equal(
                    component.as_os_str(),
                    mount_component.as_os_str(),
                    case_sensitive,
                ) => {}
            _ => return false,
        }
    }
    true
}

fn component_equal(a: &std::ffi::OsStr, b: &std::ffi::OsStr, case_sensitive: bool) -> bool {
    if case_sensitive {
        a == b
    } else {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }
}

fn mount_points_summary(disks: &Disks) -> String {
    disks
        .list()
        .iter()
        .map(|disk| disk.mount_point().display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Outcome of the pre-admission free-space check for one job's target output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiskSpaceVerdict {
    /// Enough free space was reported for the job to be admitted.
    Healthy,
    /// Free space is at or below the safety margin; the job must not start.
    Insufficient {
        available_bytes: u64,
        margin_bytes: u64,
        path: PathBuf,
    },
    /// Free space could not be determined; the job must not start (fail closed).
    Unavailable { message: String, path: PathBuf },
}

/// The Stage 3.3 gate checked by the scheduler *before* it acquires capacity
/// for a new job.
///
/// Deliberately stateless beyond its injected source and margin: every
/// admission performs a fresh, live query, so critical exhaustion mid-batch is
/// caught on the next attempted admission.
#[derive(Debug, Clone)]
pub struct DiskAdmissionGate {
    source: Arc<dyn DiskSpaceSource>,
    margin_bytes: u64,
}

impl DiskAdmissionGate {
    pub fn new(source: Arc<dyn DiskSpaceSource>, margin_bytes: u64) -> Self {
        Self {
            source,
            margin_bytes,
        }
    }

    /// The conservative free-space floor this gate enforces.
    pub fn margin_bytes(&self) -> u64 {
        self.margin_bytes
    }

    /// Checks whether `output_path`'s filesystem has enough free space for the
    /// job to be admitted (strictly above the safety margin).
    pub fn admit(&self, output_path: &Path) -> DiskSpaceVerdict {
        match self.source.available_bytes(output_path) {
            Ok(available_bytes) if available_bytes > self.margin_bytes => DiskSpaceVerdict::Healthy,
            Ok(available_bytes) => DiskSpaceVerdict::Insufficient {
                available_bytes,
                margin_bytes: self.margin_bytes,
                path: output_path.to_path_buf(),
            },
            Err(error) => DiskSpaceVerdict::Unavailable {
                message: error.to_string(),
                path: output_path.to_path_buf(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a ResourceProfile with given CPU count and RAM in bytes.
    fn profile(cpu_threads: usize, available_memory_bytes: u64) -> ResourceProfile {
        ResourceProfile {
            logical_cpu_threads: cpu_threads,
            available_memory_bytes,
        }
    }

    // Constants for RAM thresholds in bytes.
    const GB: u64 = 1024 * 1024 * 1024;

    // ---- Existing tests (unchanged) ----

    #[test]
    fn detect_system_resources_can_be_called() {
        let profile = detect_system_resources();
        assert!(profile.logical_cpu_threads > 0);
        assert!(profile.available_memory_bytes > 0);
    }

    #[test]
    fn logical_cpu_threads_are_reported_on_supported_systems() {
        if !sysinfo::IS_SUPPORTED_SYSTEM {
            return;
        }
        let profile = detect_system_resources();
        assert!(profile.logical_cpu_threads > 0);
    }

    #[test]
    fn available_memory_is_non_negative_and_at_most_total_memory() {
        let mut system = System::new();
        system.refresh_memory();
        let profile = detect_system_resources();
        assert!(profile.available_memory_bytes <= system.total_memory());
    }

    #[test]
    fn profile_fields_match_sysinfo_values() {
        let system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::nothing())
                .with_memory(MemoryRefreshKind::nothing().with_ram()),
        );
        let profile = profile_from_system(&system);
        assert_eq!(profile.logical_cpu_threads, system.cpus().len());
        assert_eq!(profile.available_memory_bytes, system.available_memory());
    }

    #[test]
    fn concurrency_plan_field_values_round_trip() {
        let plan = ConcurrencyPlan {
            mode: ExecutionMode::Parallel,
            total_capacity: 4,
            resource_budget: ResourceBudget {
                cpu_threads: 8,
                available_memory_mb: 16384,
            },
        };

        assert_eq!(plan.mode, ExecutionMode::Parallel);
        assert_eq!(plan.total_capacity, 4);
        assert_eq!(plan.resource_budget.cpu_threads, 8);
        assert_eq!(plan.resource_budget.available_memory_mb, 16384);
    }

    // ---- Stage 1.3 tests: CPU tier boundaries ----

    #[test]
    fn cpu_tier_1_thread() {
        let p = profile(1, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 1);
        assert_eq!(plan.mode, ExecutionMode::Sequential);
    }

    #[test]
    fn cpu_tier_2_threads() {
        let p = profile(2, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 1);
        assert_eq!(plan.mode, ExecutionMode::Sequential);
    }

    #[test]
    fn cpu_tier_3_threads() {
        let p = profile(3, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 1);
        assert_eq!(plan.mode, ExecutionMode::Sequential);
    }

    #[test]
    fn cpu_tier_4_threads() {
        let p = profile(4, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 1);
        assert_eq!(plan.mode, ExecutionMode::Sequential);
    }

    #[test]
    fn cpu_tier_5_threads() {
        let p = profile(5, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 2);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn cpu_tier_8_threads() {
        let p = profile(8, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 2);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn cpu_tier_9_threads() {
        let p = profile(9, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 2);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn cpu_tier_16_threads() {
        let p = profile(16, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 2);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn cpu_tier_17_threads() {
        let p = profile(17, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 3);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn cpu_tier_32_threads() {
        let p = profile(32, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 3);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn cpu_tier_33_threads() {
        let p = profile(33, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 4);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    // ---- Hard cap tests ----

    #[test]
    fn hard_cap_at_64_cpus() {
        let p = profile(64, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert!(plan.total_capacity <= 4);
        assert_eq!(plan.total_capacity, 4);
    }

    #[test]
    fn hard_cap_at_128_cpus() {
        let p = profile(128, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert!(plan.total_capacity <= 4);
        assert_eq!(plan.total_capacity, 4);
    }

    #[test]
    fn hard_cap_at_256_cpus() {
        let p = profile(256, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert!(plan.total_capacity <= 4);
        assert_eq!(plan.total_capacity, 4);
    }

    // ---- RAM gate tests ----

    #[test]
    fn ram_below_4gb_forces_sequential() {
        let p = profile(32, 3 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 1);
        assert_eq!(plan.mode, ExecutionMode::Sequential);
    }

    #[test]
    fn ram_exactly_4gb_does_not_trigger_below_4gb_rule() {
        let p = profile(32, 4 * GB);
        let plan = calculate_safe_concurrency(&p);
        // 4 GB is not < 4 GB, so CPU tier applies (capped at 2 because < 8 GB)
        assert_eq!(plan.total_capacity, 2);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn ram_between_4gb_and_8gb_caps_at_2() {
        let p = profile(32, 6 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 2);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn ram_exactly_8gb_allows_cpu_tier_value() {
        let p = profile(32, 8 * GB);
        let plan = calculate_safe_concurrency(&p);
        // CPU tier for 32 threads is 3
        assert_eq!(plan.total_capacity, 3);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn ram_above_8gb_allows_cpu_tier_value() {
        let p = profile(32, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 3);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn many_cpus_below_4gb_ram_forces_sequential() {
        let p = profile(64, 3 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 1);
        assert_eq!(plan.mode, ExecutionMode::Sequential);
    }

    #[test]
    fn many_cpus_4_to_8gb_ram_caps_at_2() {
        let p = profile(64, 6 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 2);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn many_cpus_above_8gb_ram_uses_cpu_tier() {
        let p = profile(64, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 4);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    // ---- Mode selection tests ----

    #[test]
    fn capacity_1_is_sequential() {
        let p = profile(1, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.mode, ExecutionMode::Sequential);
    }

    #[test]
    fn capacity_2_is_parallel() {
        let p = profile(5, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn capacity_3_is_parallel() {
        let p = profile(17, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn capacity_4_is_parallel() {
        let p = profile(33, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    // ---- Stage 4 fail-safe: conservative when the machine is unknown ----
    //
    // architecture_fix Stage 4: when a resource observation is missing or
    // nonsensical (0 threads / 0 available RAM), the planner must fall back to
    // *less* concurrency, never more. These tests pin that unknown
    // measurements resolve to Sequential (capacity 1) — the same safe result
    // the tier table and RAM gate already converged on, now explicit and
    // locked in so a future edit cannot accidentally turn a 0 into a
    // high-concurrency plan.

    #[test]
    fn zero_cpu_threads_resolves_conservatively() {
        let p = profile(0, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 1);
        assert_eq!(plan.mode, ExecutionMode::Sequential);
    }

    #[test]
    fn zero_available_ram_resolves_conservatively() {
        let p = profile(16, 0);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 1);
        assert_eq!(plan.mode, ExecutionMode::Sequential);
    }

    #[test]
    fn missing_ram_and_cpu_never_yield_parallel() {
        let p = profile(0, 0);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 1);
        assert_eq!(plan.mode, ExecutionMode::Sequential);
    }

    // ---- architecture_fix Stage 1/2/6: no per-job FFmpeg thread hint ----

    #[test]
    fn plan_carries_no_per_job_ffmpeg_thread_hint() {
        // ConcurrencyPlan owns only *admission* concurrency (a cost-unit
        // capacity). It deliberately exposes no per-job FFmpeg thread figure
        // for production to forward to `-threads` (FFmpeg AUTO is the
        // invariant); the struct has no such field by construction.
        let p = profile(33, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 4);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
        assert_eq!(plan.resource_budget.cpu_threads, 33);
        assert_eq!(plan.resource_budget.available_memory_mb, 16384);
    }

    // ---- Edge case tests ----

    #[test]
    fn edge_1_cpu_low_ram() {
        let p = profile(1, 2 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 1);
        assert_eq!(plan.mode, ExecutionMode::Sequential);
    }

    #[test]
    fn edge_1_cpu_high_ram() {
        let p = profile(1, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 1);
        assert_eq!(plan.mode, ExecutionMode::Sequential);
    }

    #[test]
    fn edge_4_cpus_high_ram() {
        let p = profile(4, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 1);
        assert_eq!(plan.mode, ExecutionMode::Sequential);
    }

    #[test]
    fn edge_5_cpus_high_ram() {
        let p = profile(5, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 2);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn edge_8_cpus_high_ram() {
        let p = profile(8, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 2);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn edge_9_cpus_high_ram() {
        let p = profile(9, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 2);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn edge_16_cpus_high_ram() {
        let p = profile(16, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 2);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn edge_17_cpus_high_ram() {
        let p = profile(17, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 3);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn edge_32_cpus_high_ram() {
        let p = profile(32, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 3);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn edge_33_cpus_high_ram() {
        let p = profile(33, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 4);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    #[test]
    fn edge_64_plus_cpus_high_ram() {
        let p = profile(100, 32 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 4);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    // ---- Resource budget tests ----

    #[test]
    fn resource_budget_populated_correctly() {
        let p = profile(8, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.resource_budget.cpu_threads, 8);
        assert_eq!(plan.resource_budget.available_memory_mb, 16384);
    }

    // ---- Determinism test ----

    #[test]
    fn same_profile_produces_same_plan() {
        let p = profile(16, 8 * GB);
        let plan1 = calculate_safe_concurrency(&p);
        let plan2 = calculate_safe_concurrency(&p);
        assert_eq!(plan1, plan2);
    }

    // ---- Stage 3.1: live RAM gating at batch start ----
    //
    // Scenario A (normal/high RAM), Scenario B (low RAM) and Scenario C (RAM
    // changes *between* batch starts) are the Stage 3.1 acceptance cases. The
    // A/B scenarios pin the planner's behavior for a given live RAM state; the
    // C tests exercise the real batch-start seam (`resolve_batch_start_plan`)
    // and prove that consecutive batch starts perform a fresh detection and
    // derive a fresh plan — Batch B never inherits Batch A's RAM state or plan.

    /// Scenario A — normal/high available RAM at batch start: the CPU tier is
    /// allowed, subject only to the existing gates.
    #[test]
    fn scenario_a_normal_ram_allows_cpu_capacity() {
        let p = profile(8, 16 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 2);
        assert_eq!(plan.mode, ExecutionMode::Parallel);

        let p = profile(33, 32 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 4);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    /// Scenario B — low available RAM at batch start: the Stage 1.3 RAM gate
    /// reduces capacity appropriately.
    #[test]
    fn scenario_b_low_ram_gates_capacity() {
        // < 4 GiB available forces Sequential even on a high-core machine.
        let p = profile(64, 3 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 1);
        assert_eq!(plan.mode, ExecutionMode::Sequential);

        // 4–8 GiB caps the CPU-tier value at 2.
        let p = profile(64, 6 * GB);
        let plan = calculate_safe_concurrency(&p);
        assert_eq!(plan.total_capacity, 2);
        assert_eq!(plan.mode, ExecutionMode::Parallel);
    }

    /// Scenario C (most important) — RAM changes between batch starts.
    ///
    /// Each batch-start resolution goes through the real production seam
    /// (`resolve_batch_start_plan`), which calls the live resource provider
    /// exactly once per call. Batch A resolves from RAM state A (5 GiB →
    /// RAM-gated to 2), Batch B from RAM state B (3 GiB → Sequential), and
    /// Batch C from RAM state C (12 GiB → back to the CPU-tier value). A stale
    /// observation from Batch A must never leak into Batch B or C.
    #[test]
    fn scenario_c_batch_b_does_not_inherit_batch_a_ram_state() {
        // One provider sequence stands in for the OS across three batch starts.
        // 6 threads -> CPU tier 2; 5 GiB -> < 8 GiB so the RAM gate also caps
        // at 2; 3 GiB -> < 4 GiB forces Sequential; 12 GiB -> >= 8 GiB so the
        // CPU-tier value applies unmodified.
        let states = [profile(6, 5 * GB), profile(6, 3 * GB), profile(6, 12 * GB)];
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let detect = move || {
            let idx = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            assert!(
                idx < states.len(),
                "detection called an unexpected number of times"
            );
            states[idx]
        };

        let batch_a = resolve_batch_start_plan(&detect);
        let batch_b = resolve_batch_start_plan(&detect);
        let batch_c = resolve_batch_start_plan(&detect);

        assert_eq!(
            batch_a.total_capacity, 2,
            "Batch A sees RAM state A (RAM-gated to 2)"
        );
        assert_eq!(batch_a.mode, ExecutionMode::Parallel);
        assert_eq!(
            batch_b.total_capacity, 1,
            "Batch B must see RAM state B and be RAM-gated down to Sequential"
        );
        assert_eq!(batch_b.mode, ExecutionMode::Sequential);
        assert_eq!(
            batch_c.total_capacity, 2,
            "Batch C must see recovered RAM state C and replan upward again"
        );
        assert_eq!(batch_c.mode, ExecutionMode::Parallel);

        // Both directions are demonstrably exercised: higher -> lower for
        // B after A, and lower -> higher for C after B. The planner responds to
        // changing system state rather than merely always picking the safer low
        // capacity.
        assert!(batch_b.total_capacity < batch_a.total_capacity);
        assert!(batch_c.total_capacity > batch_b.total_capacity);
    }

    /// The other Scenario C direction as a guarded edge: a batch start that
    /// resolves a *lower* capped capacity (huge CPU tier, 6 GiB RAM -> 2) is
    /// followed by one whose RAM recovered (huge CPU tier, 32 GiB RAM -> 4),
    /// proving the RAM gate itself is recomputed per batch and never sticky.
    #[test]
    fn scenario_c_ram_gate_releases_when_ram_recovers() {
        let states = [profile(33, 6 * GB), profile(33, 32 * GB)];
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let detect = move || {
            let idx = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            states[idx]
        };

        let batch_a = resolve_batch_start_plan(&detect);
        let batch_b = resolve_batch_start_plan(&detect);

        assert_eq!(
            batch_a.total_capacity, 2,
            "6 GiB caps the CPU-tier value 4 at 2"
        );
        assert_eq!(
            batch_b.total_capacity, 4,
            "recovered RAM must let the next batch use its full CPU-tier capacity"
        );
        assert!(batch_b.total_capacity > batch_a.total_capacity);
    }

    /// The batch-start seam must perform exactly one fresh detection per
    /// resolution: it is not allowed to cache a snapshot or reuse a previous
    /// plan between calls.
    #[test]
    fn batch_start_resolution_runs_one_fresh_detection_per_call() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let detect = {
            let calls = calls.clone();
            move || {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                profile(6, 12 * GB)
            }
        };

        // Three independent batch-start resolutions:
        let _a = resolve_batch_start_plan(&detect);
        let _b = resolve_batch_start_plan(&detect);
        let _c = resolve_batch_start_plan(&detect);
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            3,
            "each batch-start plan resolution must re-detect system resources exactly once"
        );
    }

    /// Resolutions are fully independent: the plan returned for a later batch
    /// shares no state with an earlier one (a fresh `ResourceProfile`, a fresh
    /// `ConcurrencyPlan`, a fresh capacity value). This is the planner-side
    /// guarantee behind "no stale capacity leaks between batches".
    #[test]
    fn batch_start_plans_are_fully_independent() {
        let detect = || profile(6, 12 * GB);
        let batch_a = resolve_batch_start_plan(detect);
        let batch_b = resolve_batch_start_plan(detect);

        // Same conditions -> same plan (determinism), but the objects are
        // derived afresh each time (e.g. the resource budget is the live MB
        // conversion of the just-detected bytes, never a stored snapshot).
        assert_eq!(batch_a, batch_b);
        assert_eq!(batch_a.resource_budget.available_memory_mb, 12288);
        assert_eq!(batch_b.resource_budget.available_memory_mb, 12288);
    }

    // ---- Stage 3.3 — disk admission gate unit tests ----

    #[derive(Debug)]
    enum Probe {
        Bytes(u64),
        Failed(&'static str),
    }

    #[derive(Debug)]
    struct FixedDiskSpace {
        probe: Probe,
    }

    impl DiskSpaceSource for FixedDiskSpace {
        fn available_bytes(&self, _path: &Path) -> io::Result<u64> {
            match self.probe {
                Probe::Bytes(bytes) => Ok(bytes),
                Probe::Failed(message) => Err(io::Error::other(message)),
            }
        }
    }

    fn gate(probe: Probe) -> DiskAdmissionGate {
        DiskAdmissionGate::new(Arc::new(FixedDiskSpace { probe }), DISK_SAFETY_MARGIN_BYTES)
    }

    #[test]
    fn disk_gate_admits_when_above_safety_margin() {
        let gate = gate(Probe::Bytes(DISK_SAFETY_MARGIN_BYTES + 1));
        assert_eq!(
            gate.admit(Path::new("C:\\out\\clip.mp4")),
            DiskSpaceVerdict::Healthy
        );
    }

    #[test]
    fn disk_gate_rejects_when_exactly_at_margin() {
        // Conservative strict threshold: exactly the margin is NOT enough.
        let gate = gate(Probe::Bytes(DISK_SAFETY_MARGIN_BYTES));
        let verdict = gate.admit(Path::new("C:\\out\\clip.mp4"));
        assert_eq!(
            verdict,
            DiskSpaceVerdict::Insufficient {
                available_bytes: DISK_SAFETY_MARGIN_BYTES,
                margin_bytes: DISK_SAFETY_MARGIN_BYTES,
                path: PathBuf::from("C:\\out\\clip.mp4"),
            }
        );
    }

    #[test]
    fn disk_gate_rejects_when_below_margin_with_reported_numbers() {
        let gate = gate(Probe::Bytes(1234));
        match gate.admit(Path::new("D:\\out\\clip.mp4")) {
            DiskSpaceVerdict::Insufficient {
                available_bytes,
                margin_bytes,
                path,
            } => {
                assert_eq!(available_bytes, 1234);
                assert_eq!(margin_bytes, DISK_SAFETY_MARGIN_BYTES);
                assert_eq!(path, PathBuf::from("D:\\out\\clip.mp4"));
            }
            other => panic!("expected Insufficient, got {:?}", other),
        }
    }

    #[test]
    fn disk_gate_fails_closed_when_free_space_cannot_be_queried() {
        let gate = gate(Probe::Failed("volume offline"));
        match gate.admit(Path::new("E:\\out\\clip.mp4")) {
            DiskSpaceVerdict::Unavailable { message, path } => {
                assert!(
                    message.contains("volume offline"),
                    "reason preserved: {}",
                    message
                );
                assert_eq!(path, PathBuf::from("E:\\out\\clip.mp4"));
            }
            other => panic!("expected Unavailable (fail closed), got {:?}", other),
        }
    }

    #[test]
    fn disk_gate_health_is_a_strict_ordering_boundary() {
        let above = gate(Probe::Bytes(DISK_SAFETY_MARGIN_BYTES + 1));
        let at = gate(Probe::Bytes(DISK_SAFETY_MARGIN_BYTES));
        assert_eq!(
            above.admit(Path::new("C:\\x.mp4")),
            DiskSpaceVerdict::Healthy
        );
        assert_ne!(at.admit(Path::new("C:\\x.mp4")), DiskSpaceVerdict::Healthy);
    }

    #[test]
    fn disk_mount_matching_uses_longest_prefix() {
        // Case-sensitive match on Unix-style mounts.
        let mounts = vec![
            PathBuf::from("/mnt"),
            PathBuf::from("/mnt/vol2"),
            PathBuf::from("/home"),
        ];
        let chosen = choose_mount(&mounts, Path::new("/mnt/vol2/out/clip.mp4"));
        assert_eq!(chosen, Some(Path::new("/mnt/vol2")));
        assert_eq!(choose_mount(&mounts, Path::new("/tmp/x.mp4")), None);
    }

    #[test]
    fn disk_mount_matching_folds_windows_drive_case() {
        if cfg!(not(windows)) {
            return; // Windows-specific: mount matching is case-insensitive there.
        }
        let mounts = vec![PathBuf::from("D:\\")];
        let chosen = choose_mount(&mounts, Path::new("d:\\My Programs\\out\\clip.mp4"));
        assert_eq!(chosen, Some(Path::new("D:\\")));
        assert_eq!(choose_mount(&mounts, Path::new("C:\\out\\clip.mp4")), None);
    }

    #[test]
    fn system_disk_space_source_queries_the_temp_filesystem() {
        let probe = SystemDiskSpaceSource;
        let free = probe
            .available_bytes(&std::env::temp_dir())
            .expect("system disk probe must resolve the temp filesystem");
        assert!(
            free > 0,
            "reported free space for temp dir must be positive"
        );
    }
}
