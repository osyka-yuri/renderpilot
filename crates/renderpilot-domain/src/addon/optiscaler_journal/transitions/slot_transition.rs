fn validate_action_slot_transition(
    current: &OperationEffect,
    current_slots: &PrivateArtifactSlots,
    next: &OperationEffect,
    next_slots: &PrivateArtifactSlots,
) -> Result<(), OptiScalerJournalError> {
    let invalid = || {
        OptiScalerJournalError::Invalid(
            "operation transition changed private artifact slots illegally",
        )
    };
    let absent =
        |observation: &DurableObservation| matches!(observation, DurableObservation::Absent);
    let file_or_absent = |observation: &DurableObservation| {
        matches!(
            observation,
            DurableObservation::Absent | DurableObservation::File { .. }
        )
    };
    let directory_or_absent = |observation: &DurableObservation| {
        matches!(
            observation,
            DurableObservation::Absent | DurableObservation::Directory { .. }
        )
    };
    let unchanged = || {
        current_slots.custody() == next_slots.custody()
            && current_slots.stage() == next_slots.stage()
            && current_slots.discard() == next_slots.discard()
    };
    let all_absent = |slots: &PrivateArtifactSlots| {
        absent(slots.custody()) && absent(slots.stage()) && absent(slots.discard())
    };

    let valid = match (current, next) {
        (OperationEffect::Write(left), OperationEffect::Write(right)) => {
            use WriteState::*;
            match (left.state(), right.state()) {
                (Planned, StageIntent { .. })
                | (Staged { .. }, CaptureIntent { .. })
                | (Captured { .. }, PublishIntent { .. }) => unchanged(),
                (StageIntent { .. }, Staged { stage }) => {
                    all_absent(current_slots)
                        && next_slots.stage() == stage
                        && absent(next_slots.custody())
                        && absent(next_slots.discard())
                }
                (
                    CaptureIntent { stage: left_stage },
                    Captured {
                        stage: right_stage,
                        custody,
                    },
                ) => {
                    left_stage == right_stage
                        && current_slots.stage() == left_stage
                        && absent(current_slots.custody())
                        && absent(current_slots.discard())
                        && next_slots.stage() == right_stage
                        && next_slots.custody() == custody
                        && absent(next_slots.discard())
                }
                (
                    PublishIntent { stage, custody },
                    Applied {
                        custody: right_custody,
                        ..
                    },
                ) => {
                    current_slots.stage() == stage
                        && current_slots.custody() == custody
                        && absent(current_slots.discard())
                        && absent(next_slots.stage())
                        && next_slots.custody() == right_custody
                        && absent(next_slots.discard())
                }
                (Applied { .. }, DiscardIntent { .. }) => {
                    file_or_absent(current_slots.custody())
                        && absent(current_slots.stage())
                        && absent(current_slots.discard())
                        && all_absent(next_slots)
                }
                (Applied { .. }, RestoreIntent { .. }) => {
                    matches!(current_slots.custody(), DurableObservation::File { .. })
                        && unchanged()
                }
                (DiscardIntent { .. }, PostimageDiscarded { .. })
                | (PostimageDiscarded { .. }, Preserved) => {
                    all_absent(current_slots) && all_absent(next_slots)
                }
                (RestoreIntent { .. }, Preserved) => {
                    matches!(current_slots.custody(), DurableObservation::File { .. })
                        && absent(current_slots.stage())
                        && absent(current_slots.discard())
                        && all_absent(next_slots)
                }
                (StageIntent { .. }, Preserved) => {
                    all_absent(current_slots)
                        && file_or_absent(next_slots.stage())
                        && absent(next_slots.custody())
                        && absent(next_slots.discard())
                }
                (Staged { stage }, Preserved) => {
                    current_slots.stage() == stage
                        && absent(current_slots.custody())
                        && absent(current_slots.discard())
                        && next_slots.stage() == stage
                        && absent(next_slots.custody())
                        && absent(next_slots.discard())
                }
                (CaptureIntent { stage }, Preserved) => {
                    current_slots.stage() == stage
                        && absent(current_slots.custody())
                        && absent(current_slots.discard())
                        && next_slots.stage() == stage
                        && file_or_absent(next_slots.custody())
                        && absent(next_slots.discard())
                }
                (Captured { stage, custody } | PublishIntent { stage, custody }, Preserved) => {
                    current_slots.stage() == stage
                        && current_slots.custody() == custody
                        && absent(current_slots.discard())
                        && next_slots.stage() == stage
                        && next_slots.custody() == custody
                        && absent(next_slots.discard())
                }
                (Planned, Preserved) => all_absent(current_slots) && all_absent(next_slots),
                _ => false,
            }
        }
        (OperationEffect::Delete(left), OperationEffect::Delete(right)) => {
            use DeleteState::*;
            match (left.state(), right.state()) {
                (Planned, CaptureIntent) => unchanged(),
                (CaptureIntent, Captured { custody }) => {
                    all_absent(current_slots)
                        && next_slots.custody() == custody
                        && absent(next_slots.stage())
                        && absent(next_slots.discard())
                }
                (
                    Captured { custody },
                    Applied {
                        custody: right_custody,
                    },
                ) => custody == right_custody && unchanged(),
                (Applied { custody }, RestoreIntent { preimage }) => {
                    custody == preimage
                        && matches!(current_slots.custody(), DurableObservation::File { .. })
                        && unchanged()
                }
                (RestoreIntent { .. }, Preserved) => {
                    matches!(current_slots.custody(), DurableObservation::File { .. })
                        && absent(current_slots.stage())
                        && absent(current_slots.discard())
                        && all_absent(next_slots)
                }
                (CaptureIntent | Planned, Preserved) => {
                    all_absent(current_slots) && all_absent(next_slots)
                }
                _ => false,
            }
        }
        (OperationEffect::Verify(_), OperationEffect::Verify(_))
        | (OperationEffect::Relocate(_), OperationEffect::Relocate(_))
        | (
            OperationEffect::PostCommitRemoveDirectory(_),
            OperationEffect::PostCommitRemoveDirectory(_),
        ) => all_absent(current_slots) && all_absent(next_slots),
        (OperationEffect::CreateDirectory(left), OperationEffect::CreateDirectory(right)) => {
            use CreateDirectoryState::*;
            match (left.state(), right.state()) {
                (Planned, StageIntent) => unchanged(),
                (StageIntent, Staged { stage }) => {
                    all_absent(current_slots)
                        && next_slots.stage() == stage
                        && absent(next_slots.custody())
                        && absent(next_slots.discard())
                }
                (Staged { stage: left_stage }, PublishIntent { stage: right_stage }) => {
                    left_stage == right_stage && unchanged()
                }
                (PublishIntent { stage }, Applied { .. }) => {
                    current_slots.stage() == stage
                        && absent(current_slots.custody())
                        && absent(current_slots.discard())
                        && absent(next_slots.stage())
                        && absent(next_slots.custody())
                        && absent(next_slots.discard())
                }
                (Applied { .. }, DiscardIntent { .. })
                | (DiscardIntent { .. }, PostimageDiscarded { .. })
                | (PostimageDiscarded { .. }, Preserved) => {
                    all_absent(current_slots) && all_absent(next_slots)
                }
                (StageIntent, Preserved) => {
                    all_absent(current_slots)
                        && directory_or_absent(next_slots.stage())
                        && absent(next_slots.custody())
                        && absent(next_slots.discard())
                }
                (Staged { stage } | PublishIntent { stage }, Preserved) => {
                    current_slots.stage() == stage
                        && absent(current_slots.custody())
                        && absent(current_slots.discard())
                        && next_slots.stage() == stage
                        && absent(next_slots.custody())
                        && absent(next_slots.discard())
                }
                (Planned, Preserved) => all_absent(current_slots) && all_absent(next_slots),
                _ => false,
            }
        }
        _ => false,
    };
    valid.ok_or_else(invalid)
}
