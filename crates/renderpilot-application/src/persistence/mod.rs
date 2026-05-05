mod backup_id;
mod backup_record;
mod metadata_json;
mod operation_item_record;
mod operation_kind;
mod operation_record;
mod operation_status;
mod unix_timestamp_millis;

use crate::{AppError, AppResult};

fn normalize_non_empty_text(
	value: impl Into<String>,
	field_name: &'static str,
) -> AppResult<String> {
	let value = value.into();
	let trimmed = value.trim();

	if trimmed.is_empty() {
		return Err(AppError::invalid_input(format!(
			"{field_name} must not be empty"
		)));
	}

	Ok(trimmed.to_owned())
}

pub use backup_id::BackupId;
pub use backup_record::BackupRecord;
pub use metadata_json::MetadataJson;
pub use operation_item_record::OperationItemRecord;
pub use operation_kind::OperationKind;
pub use operation_record::OperationRecord;
pub use operation_status::OperationStatus;
pub use unix_timestamp_millis::UnixTimestampMillis;
