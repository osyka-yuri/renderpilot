use std::collections::{HashMap, HashSet};

use renderpilot_application::{
    AppError, AppResult, BackupRecord, OperationItemRecord, OperationRecord,
};
use renderpilot_domain::{ComponentFile, ComponentId, GraphicsComponent, PathRef};

pub(super) trait CatalogUpdateItem {
    fn catalog_component_id(&self) -> &ComponentId;

    fn catalog_file_path(&self) -> &PathRef;

    fn updated_component_file(&self, file: &ComponentFile) -> ComponentFile;

    fn missing_catalog_entry_error(&self) -> AppError;
}

pub(super) fn rebuild_component_catalog<T>(
    components: Vec<GraphicsComponent>,
    prepared_items: &[T],
) -> AppResult<Vec<GraphicsComponent>>
where
    T: CatalogUpdateItem,
{
    let items_by_key = prepared_items
        .iter()
        .map(|prepared_item| {
            (
                CatalogFileKey::new(
                    prepared_item.catalog_component_id().as_str(),
                    prepared_item.catalog_file_path().as_str(),
                ),
                prepared_item,
            )
        })
        .collect::<HashMap<_, _>>();
    let mut matched_files = HashSet::<CatalogFileKey>::with_capacity(items_by_key.len());
    let updated_components = components
        .into_iter()
        .map(|component| rebuild_component(component, &items_by_key, &mut matched_files))
        .collect();

    ensure_all_items_matched(&items_by_key, &matched_files)?;

    Ok(updated_components)
}

pub(super) fn backup_lookup_map(
    backups: &[BackupRecord],
) -> HashMap<BackupLookupKey, BackupRecord> {
    backups
        .iter()
        .map(|backup| (BackupLookupKey::from_backup(backup), backup.clone()))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CatalogFileKey {
    component_id: String,
    file_path: String,
}

impl CatalogFileKey {
    fn new(component_id: &str, file_path: &str) -> Self {
        Self {
            component_id: component_id.to_owned(),
            file_path: file_path.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct BackupLookupKey {
    operation_id: String,
    source_path: String,
}

impl BackupLookupKey {
    pub(super) fn from_operation_item(
        operation: &OperationRecord,
        item: &OperationItemRecord,
    ) -> Self {
        Self {
            operation_id: operation.id.as_str().to_owned(),
            source_path: item.source_path.as_str().to_owned(),
        }
    }

    pub(super) fn from_backup(backup: &BackupRecord) -> Self {
        Self {
            operation_id: backup.operation_id.as_str().to_owned(),
            source_path: backup.original_path.as_str().to_owned(),
        }
    }
}

fn rebuild_component<T>(
    component: GraphicsComponent,
    items_by_key: &HashMap<CatalogFileKey, &T>,
    matched_files: &mut HashSet<CatalogFileKey>,
) -> GraphicsComponent
where
    T: CatalogUpdateItem,
{
    let mut updated_component = GraphicsComponent::new(
        component.id().clone(),
        component.game_id().clone(),
        component.kind(),
        component.technology(),
        component.swappability(),
    );

    for file in component.files() {
        let key = CatalogFileKey::new(component.id().as_str(), file.path().as_str());

        if let Some(prepared_item) = items_by_key.get(&key) {
            matched_files.insert(key);
            updated_component =
                updated_component.with_file(prepared_item.updated_component_file(file));
        } else {
            updated_component = updated_component.with_file(file.clone());
        }
    }

    updated_component
}

fn ensure_all_items_matched<T>(
    items_by_key: &HashMap<CatalogFileKey, &T>,
    matched_files: &HashSet<CatalogFileKey>,
) -> AppResult<()>
where
    T: CatalogUpdateItem,
{
    for (key, prepared_item) in items_by_key {
        if !matched_files.contains(key) {
            return Err(prepared_item.missing_catalog_entry_error());
        }
    }

    Ok(())
}
