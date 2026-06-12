use renderpilot_domain::OperationId;

use super::{OperationItemRecord, OperationRecord, OperationStatus, UnixTimestampMillis};
use crate::{AppError, AppResult};

/// One **operation journal entry**: the persisted operation header together with every
/// child item row belonging to that operation.
///
/// Application code should treat this as the unit of persistence: the
/// [`OperationRepository`](crate::ports::OperationRepository) port saves
/// and loads headers and items together so the journal cannot be left with a new header
/// and stale items, or vice versa.
///
/// # Invariant
///
/// Every [`OperationItemRecord::operation_id`] must equal [`OperationRecord::id`] for the
/// header held in the same entry. Field access is private so this invariant cannot be
/// broken by mutating `operation` or `items` after construction.
///
/// The order of items is significant: adapters must preserve slice order on write and
/// return items in a stable, deterministic order on read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationJournalEntry {
    operation: OperationRecord,
    items: Vec<OperationItemRecord>,
}

impl OperationJournalEntry {
    /// Constructs an operation journal entry after validating that every item belongs
    /// to the supplied operation header.
    pub fn try_new(operation: OperationRecord, items: Vec<OperationItemRecord>) -> AppResult<Self> {
        Self::validate_items_match_header(&operation, &items)?;

        Ok(Self { operation, items })
    }

    /// Validates that this entry satisfies the same invariant as [`Self::try_new`].
    pub fn validate(&self) -> AppResult<()> {
        Self::validate_items_match_header(&self.operation, &self.items)
    }

    /// Returns a new entry with the same header/items moved to one shared state.
    pub fn with_state(
        &self,
        status: OperationStatus,
        completed_at: Option<UnixTimestampMillis>,
    ) -> AppResult<Self> {
        let operation = self
            .operation
            .clone()
            .with_state(status.clone(), completed_at);
        let items = self
            .items
            .iter()
            .cloned()
            .map(|item| item.with_status(status.clone()))
            .collect();

        Self::try_new(operation, items)
    }

    /// Validates that every item's [`OperationItemRecord::operation_id`] equals
    /// [`OperationRecord::id`].
    ///
    /// Shared by [`Self::try_new`], [`Self::validate`], and storage adapters before write.
    pub fn validate_items_match_header(
        operation: &OperationRecord,
        items: &[OperationItemRecord],
    ) -> AppResult<()> {
        match first_operation_id_mismatch(operation, items) {
            Some(mismatch) => Err(mismatch.into_error()),
            None => Ok(()),
        }
    }

    /// Returns the operation header.
    #[must_use]
    pub fn operation(&self) -> &OperationRecord {
        &self.operation
    }

    /// Returns all item rows in stable persistence order.
    #[must_use]
    pub fn items(&self) -> &[OperationItemRecord] {
        &self.items
    }

    /// Identifier of the owning operation.
    #[must_use]
    pub fn operation_id(&self) -> &OperationId {
        &self.operation.id
    }

    /// Number of item rows in this entry.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` when there are no item rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Consumes the entry and returns the header and item rows.
    ///
    /// The returned pair satisfies the same invariant that [`Self::try_new`] enforces
    /// at the moment of the call. After unpacking, the caller owns raw records again:
    /// mutating them can break the invariant unless the pair is passed back through
    /// [`Self::try_new`] or validated before persistence.
    #[must_use]
    pub fn into_parts(self) -> (OperationRecord, Vec<OperationItemRecord>) {
        (self.operation, self.items)
    }
}

#[derive(Debug, Clone, Copy)]
struct OperationIdMismatch<'a> {
    item_index: usize,
    actual_id: &'a OperationId,
    expected_id: &'a OperationId,
}

impl OperationIdMismatch<'_> {
    fn into_error(self) -> AppError {
        AppError::invalid_input(format!(
            "operation journal item at index {} has operation_id ({}) but expected operation id ({})",
            self.item_index,
            self.actual_id.as_str(),
            self.expected_id.as_str(),
        ))
    }
}

fn first_operation_id_mismatch<'a>(
    operation: &'a OperationRecord,
    items: &'a [OperationItemRecord],
) -> Option<OperationIdMismatch<'a>> {
    let expected_id = &operation.id;

    items.iter().enumerate().find_map(|(item_index, item)| {
        let actual_id = &item.operation_id;

        (actual_id != expected_id).then_some(OperationIdMismatch {
            item_index,
            actual_id,
            expected_id,
        })
    })
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{ComponentId, GameId, OperationId, PathRef};

    use super::{OperationItemRecord, OperationJournalEntry, OperationRecord};
    use crate::{AppErrorKind, OperationKind, OperationStatus, UnixTimestampMillis};

    fn sample_operation(id: &str) -> OperationRecord {
        OperationRecord::new(
            OperationId::new(id).expect("operation id"),
            GameId::new("game:g1").expect("game id"),
            OperationKind::ReplaceComponent,
            OperationStatus::Planned,
            UnixTimestampMillis::new(1).expect("timestamp"),
        )
    }

    fn sample_item(operation_id: &OperationId, suffix: &str) -> OperationItemRecord {
        OperationItemRecord::new(
            operation_id.clone(),
            ComponentId::new(format!("component:{suffix}")).expect("component id"),
            PathRef::new(format!("C:/x/{suffix}.dll")).expect("path"),
            OperationStatus::Planned,
        )
    }

    #[test]
    fn try_new_accepts_empty_items() {
        let op = sample_operation("op:1");

        let entry = OperationJournalEntry::try_new(op.clone(), vec![]).expect("empty items");

        assert_eq!(entry.operation(), &op);
        assert!(entry.is_empty());
        assert_eq!(entry.len(), 0);
    }

    #[test]
    fn try_new_accepts_items_with_matching_operation_id() {
        let op = sample_operation("op:match");
        let id = op.id.clone();
        let items = vec![sample_item(&id, "a"), sample_item(&id, "b")];

        let entry = OperationJournalEntry::try_new(op.clone(), items.clone()).expect("valid");

        assert_eq!(entry.operation(), &op);
        assert_eq!(entry.operation_id(), &op.id);
        assert_eq!(entry.items(), items.as_slice());
        assert_eq!(entry.len(), 2);
        assert!(!entry.is_empty());
    }

    #[test]
    fn try_new_rejects_mismatched_item_operation_id() {
        let op = sample_operation("op:correct");
        let wrong_id = OperationId::new("op:wrong").expect("operation id");
        let item = sample_item(&wrong_id, "a");

        let error = OperationJournalEntry::try_new(op, vec![item]).expect_err("mismatch");

        assert!(
            error.is(&AppErrorKind::InvalidInput),
            "expected invalid_input, got {:?}",
            error.kind()
        );

        let msg = error.message();

        assert!(
            msg.contains("index 0"),
            "message should include item index: {msg}"
        );
        assert!(
            msg.contains("op:wrong") && msg.contains("op:correct"),
            "message should name both ids: {msg}"
        );
    }

    #[test]
    fn try_new_mismatch_error_names_index_of_first_bad_item() {
        let op = sample_operation("op:expected");
        let id = op.id.clone();
        let wrong = OperationId::new("op:bad").expect("operation id");
        let items = vec![
            sample_item(&id, "ok0"),
            sample_item(&id, "ok1"),
            sample_item(&wrong, "bad"),
        ];

        let error = OperationJournalEntry::try_new(op, items).expect_err("mismatch");

        assert!(error.is(&AppErrorKind::InvalidInput));

        let msg = error.message();

        assert!(
            msg.contains("index 2"),
            "message should index the failing item: {msg}"
        );
        assert!(
            msg.contains("op:bad") && msg.contains("op:expected"),
            "message should name both ids: {msg}"
        );
    }

    #[test]
    fn validate_succeeds_for_valid_entry() {
        let op = sample_operation("op:v");
        let id = op.id.clone();
        let entry = OperationJournalEntry::try_new(op, vec![sample_item(&id, "x")]).expect("entry");

        entry.validate().expect("validate");
    }

    #[test]
    fn with_state_updates_header_and_items() {
        let op = sample_operation("op:state");
        let id = op.id.clone();
        let entry =
            OperationJournalEntry::try_new(op, vec![sample_item(&id, "a"), sample_item(&id, "b")])
                .expect("entry");

        let completed_at = UnixTimestampMillis::new(42).expect("timestamp");
        let updated = entry
            .with_state(OperationStatus::Completed, Some(completed_at))
            .expect("updated entry");

        assert_eq!(updated.operation().status, OperationStatus::Completed);
        assert_eq!(updated.operation().completed_at, Some(completed_at));
        assert!(updated
            .items()
            .iter()
            .all(|item| item.status == OperationStatus::Completed));
    }

    #[test]
    fn operation_id_returns_header_id() {
        let op = sample_operation("op:id");

        let entry = OperationJournalEntry::try_new(op.clone(), vec![]).expect("entry");

        assert_eq!(entry.operation_id(), &op.id);
    }

    #[test]
    fn validate_items_match_header_accepts_empty_items() {
        let op = sample_operation("op:adapter-empty");

        OperationJournalEntry::validate_items_match_header(&op, &[])
            .expect("empty adapter payload should be valid");
    }

    #[test]
    fn validate_items_match_header_can_be_used_directly_by_adapters() {
        let op = sample_operation("op:adapter");
        let id = op.id.clone();
        let items = vec![sample_item(&id, "a"), sample_item(&id, "b")];

        OperationJournalEntry::validate_items_match_header(&op, &items)
            .expect("valid adapter payload");
    }

    #[test]
    fn validate_items_match_header_rejects_direct_adapter_mismatch() {
        let op = sample_operation("op:adapter-expected");
        let wrong_id = OperationId::new("op:adapter-wrong").expect("operation id");
        let items = vec![sample_item(&wrong_id, "a")];

        let error =
            OperationJournalEntry::validate_items_match_header(&op, &items).expect_err("mismatch");

        assert!(error.is(&AppErrorKind::InvalidInput));

        let msg = error.message();

        assert!(
            msg.contains("index 0"),
            "message should include item index: {msg}"
        );
        assert!(
            msg.contains("op:adapter-wrong") && msg.contains("op:adapter-expected"),
            "message should name both ids: {msg}"
        );
    }

    #[test]
    fn into_parts_round_trips() {
        let op = sample_operation("op:parts");
        let id = op.id.clone();
        let items = vec![sample_item(&id, "only")];
        let entry = OperationJournalEntry::try_new(op.clone(), items.clone()).expect("entry");

        let (got_op, got_items) = entry.into_parts();

        assert_eq!(got_op, op);
        assert_eq!(got_items, items);
    }
}
