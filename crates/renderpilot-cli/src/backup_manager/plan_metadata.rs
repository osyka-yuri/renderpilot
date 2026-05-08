use renderpilot_application::{
    AppError, AppResult, MetadataJson, OperationItemRecord, OperationPlan, OperationRecord,
};
use renderpilot_domain::{Sha256Hash, Version};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::fmt::Display;

const PLANNED_OPERATION_METADATA: &str = "planned operation metadata";
const PLANNED_OPERATION_ITEM_METADATA: &str = "planned operation item metadata";

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

#[derive(Debug, Serialize, Deserialize)]
struct PlannedOperationMetadataDto {
    confirmation_token: String,
}

pub(crate) fn metadata_json_for_planned_item(plan: &OperationPlan) -> AppResult<MetadataJson> {
    serialize_metadata(
        PLANNED_OPERATION_ITEM_METADATA,
        &PlannedOperationItemMetadataDto::from_plan(plan),
    )
}

pub(crate) fn metadata_json_for_planned_operation(plan: &OperationPlan) -> AppResult<MetadataJson> {
    serialize_metadata(
        PLANNED_OPERATION_METADATA,
        &PlannedOperationMetadataDto::from_plan(plan),
    )
}

pub(crate) fn planned_operation_confirmation_token(
    operation: &OperationRecord,
) -> AppResult<String> {
    let metadata_json = required_metadata(operation.metadata_json.as_ref(), || {
        format!(
            "operation is missing planned metadata for operation {}",
            operation.id.as_str()
        )
    })?;

    let dto = deserialize_metadata::<PlannedOperationMetadataDto>(
        metadata_json,
        PLANNED_OPERATION_METADATA,
        operation.id.as_str(),
    )?;

    dto.into_confirmation_token(operation.id.as_str())
}

#[cfg(test)]
pub(crate) fn planned_operation_item_metadata_json(
    original_sha256: Option<&Sha256Hash>,
    replacement_sha256: Option<&Sha256Hash>,
) -> AppResult<MetadataJson> {
    serialize_metadata(
        PLANNED_OPERATION_ITEM_METADATA,
        &PlannedOperationItemMetadataDto::from_parts(original_sha256, replacement_sha256, None),
    )
}

pub(super) fn planned_item_metadata(
    item: &OperationItemRecord,
) -> AppResult<PlannedOperationItemMetadata> {
    let metadata_json = required_metadata(item.metadata_json.as_ref(), || {
        format!(
            "operation item is missing planned metadata for operation {}",
            item.operation_id.as_str()
        )
    })?;

    let dto = deserialize_metadata::<PlannedOperationItemMetadataDto>(
        metadata_json,
        PLANNED_OPERATION_ITEM_METADATA,
        item.operation_id.as_str(),
    )?;

    dto.into_domain(item.operation_id.as_str())
}

impl PlannedOperationItemMetadataDto {
    fn from_plan(plan: &OperationPlan) -> Self {
        Self::from_parts(
            plan.original_sha256(),
            plan.replacement_sha256(),
            plan.original_version(),
        )
    }

    fn from_parts(
        original_sha256: Option<&Sha256Hash>,
        replacement_sha256: Option<&Sha256Hash>,
        original_version: Option<&Version>,
    ) -> Self {
        Self {
            original_sha256: original_sha256.map(|sha256| sha256.as_str().to_owned()),
            replacement_sha256: replacement_sha256.map(|sha256| sha256.as_str().to_owned()),
            original_version: original_version.map(|version| version.as_str().to_owned()),
        }
    }

    fn into_domain(self, operation_id: &str) -> AppResult<PlannedOperationItemMetadata> {
        Ok(PlannedOperationItemMetadata {
            original_sha256: parse_optional_field(
                "original_sha256",
                self.original_sha256,
                operation_id,
                Sha256Hash::new,
            )?,
            replacement_sha256: parse_optional_field(
                "replacement_sha256",
                self.replacement_sha256,
                operation_id,
                Sha256Hash::new,
            )?,
            original_version: parse_optional_field(
                "original_version",
                self.original_version,
                operation_id,
                Version::parse,
            )?,
        })
    }
}

impl PlannedOperationMetadataDto {
    fn from_plan(plan: &OperationPlan) -> Self {
        Self {
            confirmation_token: plan.confirmation_token().to_owned(),
        }
    }

    fn into_confirmation_token(self, operation_id: &str) -> AppResult<String> {
        if self.confirmation_token.trim().is_empty() {
            return Err(AppError::invalid_input(format!(
                "planned operation metadata has empty confirmation_token for operation {operation_id}"
            )));
        }

        Ok(self.confirmation_token)
    }
}

fn serialize_metadata<T>(metadata_kind: &str, dto: &T) -> AppResult<MetadataJson>
where
    T: Serialize,
{
    MetadataJson::new(serde_json::to_string(dto).map_err(|error| {
        AppError::provider_failed(format!("failed to serialize {metadata_kind}: {error}"))
    })?)
}

fn deserialize_metadata<T>(
    metadata_json: &MetadataJson,
    metadata_kind: &str,
    operation_id: &str,
) -> AppResult<T>
where
    T: DeserializeOwned,
{
    serde_json::from_str(metadata_json.as_str()).map_err(|error| {
        AppError::invalid_input(format!(
            "{metadata_kind} is invalid for operation {operation_id}: {error}"
        ))
    })
}

fn required_metadata(
    metadata_json: Option<&MetadataJson>,
    missing_message: impl FnOnce() -> String,
) -> AppResult<&MetadataJson> {
    metadata_json.ok_or_else(|| AppError::invalid_input(missing_message()))
}

fn parse_optional_field<T, E>(
    field_name: &'static str,
    value: Option<String>,
    operation_id: &str,
    parse: impl FnOnce(String) -> Result<T, E>,
) -> AppResult<Option<T>>
where
    E: Display,
{
    value.map(parse).transpose().map_err(|error| {
        AppError::invalid_input(format!(
            "planned operation item metadata has invalid {field_name} for operation {operation_id}: {error}"
        ))
    })
}
