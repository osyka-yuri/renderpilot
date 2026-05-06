use renderpilot_application::{
    AppError, AppResult, MetadataJson, OperationItemRecord, OperationPlan,
};
use renderpilot_domain::{Sha256Hash, Version};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub(super) struct PlannedOperationItemMetadata {
    pub(super) original_sha256: Option<Sha256Hash>,
    pub(super) replacement_sha256: Option<Sha256Hash>,
    pub(super) original_version: Option<Version>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PlannedOperationItemMetadataDto {
    original_sha256: Option<String>,
    replacement_sha256: Option<String>,
    original_version: Option<String>,
}

pub(crate) fn metadata_json_for_planned_item(plan: &OperationPlan) -> AppResult<MetadataJson> {
    metadata_json_for_hashes(
        plan.original_sha256(),
        plan.replacement_sha256(),
        plan.original_version(),
    )
}

#[cfg(test)]
pub(crate) fn planned_operation_item_metadata_json(
    original_sha256: Option<&Sha256Hash>,
    replacement_sha256: Option<&Sha256Hash>,
) -> AppResult<MetadataJson> {
    metadata_json_for_hashes(original_sha256, replacement_sha256, None)
}

pub(super) fn planned_item_metadata(
    item: &OperationItemRecord,
) -> AppResult<PlannedOperationItemMetadata> {
    let metadata_json = item.metadata_json.as_ref().ok_or_else(|| {
        AppError::invalid_input(format!(
            "operation item is missing planned hash metadata for operation {}",
            item.operation_id.as_str()
        ))
    })?;
    let dto = serde_json::from_str::<PlannedOperationItemMetadataDto>(metadata_json.as_str())
        .map_err(|error| {
            AppError::invalid_input(format!(
                "operation item planned hash metadata is invalid for operation {}: {error}",
                item.operation_id.as_str()
            ))
        })?;

    Ok(PlannedOperationItemMetadata {
        original_sha256: dto
            .original_sha256
            .map(Sha256Hash::new)
            .transpose()
            .map_err(|error| AppError::invalid_input(error.to_string()))?,
        replacement_sha256: dto
            .replacement_sha256
            .map(Sha256Hash::new)
            .transpose()
            .map_err(|error| AppError::invalid_input(error.to_string()))?,
        original_version: dto
            .original_version
            .map(Version::parse)
            .transpose()
            .map_err(|error| AppError::invalid_input(error.to_string()))?,
    })
}

fn metadata_json_for_hashes(
    original_sha256: Option<&Sha256Hash>,
    replacement_sha256: Option<&Sha256Hash>,
    original_version: Option<&Version>,
) -> AppResult<MetadataJson> {
    MetadataJson::new(
        serde_json::to_string(&PlannedOperationItemMetadataDto {
            original_sha256: original_sha256.map(|sha256| sha256.as_str().to_owned()),
            replacement_sha256: replacement_sha256.map(|sha256| sha256.as_str().to_owned()),
            original_version: original_version.map(|version| version.as_str().to_owned()),
        })
        .map_err(|error| {
            AppError::provider_failed(format!(
                "failed to serialize planned operation item hash metadata: {error}"
            ))
        })?,
    )
}
