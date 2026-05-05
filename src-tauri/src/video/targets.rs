use crate::video::types::{OutputJob, OutputTarget, TargetType};
use crate::video::validation::validate_output_job;

// SINGLE SOURCE OF TRUTH:
// All OutputTarget instances MUST be created here.
// Do not duplicate this logic elsewhere.
pub fn normalize_targets(jobs: &[OutputJob]) -> Result<Vec<OutputTarget>, String> {
    jobs.iter()
        .map(|job| {
            validate_output_job(job).map_err(|e| e.to_string())?;

            // Determine type FIRST — no fallback chains
            let (label, target_type) = if let Some(ref name) = job.preset_name {
                (name.clone(), TargetType::Platform)
            } else {
                (job.ratio.get_tag().to_string(), TargetType::AspectRatio)
            };

            Ok(OutputTarget {
                id: job.id.clone(),
                label: OutputTarget::sanitize_label(&label),
                target_type,
                job: job.clone(),
            })
        })
        .collect()
}
