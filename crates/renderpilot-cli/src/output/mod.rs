mod json;
mod text;

#[cfg(test)]
mod tests;

pub(crate) use self::json::{
    apply_operation_value, candidate_groups_value, operation_summaries_value,
    render_apply_operation_output, render_backup_output, render_candidates_output,
    render_list_artifacts_output, render_list_operations_output, render_plan_swap_output,
    render_rollback_operation_output, render_scan_folder_batch_output, render_scan_folder_output,
    rollback_operation_value,
};
pub(crate) use self::text::{render_help, render_summary, render_version, HELP_HINT};
