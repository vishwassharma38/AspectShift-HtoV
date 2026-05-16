use crate::video::types::{OutputJob, OutputTarget};
use crate::video::validation::validate_output_job;

// SINGLE SOURCE OF TRUTH:
// All OutputTarget instances MUST be created here.
// Do not duplicate this logic elsewhere.
pub fn normalize_targets(jobs: &[OutputJob]) -> Result<Vec<OutputTarget>, String> {
    jobs.iter()
        .map(|job| {
            validate_output_job(job).map_err(|e| e.to_string())?;

            Ok(OutputTarget {
                id: job.id.clone(),
                label: OutputTarget::sanitize_label(&job.selection.label),
                target_type: job.selection.source_type.clone(),
                job: job.clone(),
            })
        })
        .collect()
}
