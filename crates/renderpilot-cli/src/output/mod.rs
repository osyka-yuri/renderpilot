mod json;
mod text;

#[cfg(test)]
mod tests;

pub(crate) use self::json::{
    render_candidates_output, render_list_artifacts_output, render_list_operations_output,
    render_plan_swap_output, render_scan_folder_batch_output, render_scan_folder_output,
};
pub(crate) use self::text::{render_help, render_summary, render_version, HELP_HINT};
