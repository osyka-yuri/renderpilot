fn validate_action_payload_transition(
    current: &OperationEffect,
    next: &OperationEffect,
) -> Result<(), OptiScalerJournalError> {
    let invalid =
        || OptiScalerJournalError::Invalid("operation transition changed a durable action token");
    match (current, next) {
        (OperationEffect::Write(left), OperationEffect::Write(right)) => {
            use WriteState::*;
            match (left.state(), right.state()) {
                (StageIntent { target_digest }, Staged { stage }) => {
                    if !matches!(stage, DurableObservation::File { digest, .. } if digest == target_digest)
                    {
                        return Err(invalid());
                    }
                }
                (Staged { stage: before }, CaptureIntent { stage: after })
                | (Captured { stage: before, .. }, PublishIntent { stage: after, .. })
                    if before != after =>
                {
                    return Err(invalid());
                }
                (
                    CaptureIntent { stage: before },
                    Captured {
                        stage: after,
                        custody,
                    },
                ) => {
                    if before != after
                        || !matches!(
                            custody,
                            DurableObservation::Absent | DurableObservation::File { .. }
                        )
                    {
                        return Err(invalid());
                    }
                }
                (
                    Captured {
                        stage: left_stage,
                        custody: left_custody,
                    },
                    PublishIntent {
                        stage: right_stage,
                        custody: right_custody,
                    },
                ) if left_stage != right_stage || left_custody != right_custody => {
                    return Err(invalid());
                }
                (
                    PublishIntent {
                        stage,
                        custody: left_custody,
                    },
                    Applied {
                        live,
                        custody: right_custody,
                    },
                ) => {
                    if left_custody != right_custody
                        || !matches!(stage, DurableObservation::File { .. })
                        || !matches!(live, DurableObservation::File { .. })
                        || live.digest() != stage.digest()
                    {
                        return Err(invalid());
                    }
                }
                (Applied { live, custody: _ }, DiscardIntent { postimage, custody }) => {
                    if live != postimage || !matches!(custody, DurableObservation::Absent) {
                        return Err(invalid());
                    }
                }
                (
                    Applied { live, custody: _ },
                    RestoreIntent {
                        discard: postimage, ..
                    },
                ) => {
                    if live != postimage {
                        return Err(invalid());
                    }
                }
                (
                    DiscardIntent {
                        postimage,
                        custody: _,
                    },
                    PostimageDiscarded { discard, custody },
                ) => {
                    if postimage != discard || !matches!(custody, DurableObservation::Absent) {
                        return Err(invalid());
                    }
                }
                (RestoreIntent { preimage, discard }, Preserved)
                    if !preimage.is_exact() || !discard.is_exact() =>
                {
                    return Err(invalid());
                }
                _ => {}
            }
        }
        (OperationEffect::Delete(left), OperationEffect::Delete(right)) => {
            use DeleteState::*;
            match (left.state(), right.state()) {
                (CaptureIntent, Captured { custody })
                    if !matches!(custody, DurableObservation::File { .. }) =>
                {
                    return Err(invalid());
                }
                (Captured { custody: before }, Applied { custody: after })
                | (Applied { custody: before }, RestoreIntent { preimage: after })
                    if before != after =>
                {
                    return Err(invalid());
                }
                _ => {}
            }
        }
        (OperationEffect::Verify(_), OperationEffect::Verify(right)) => {
            if let VerifyState::Applied { observed } = right.state()
                && !observed.is_exact()
            {
                return Err(invalid());
            }
        }
        (OperationEffect::Relocate(_), OperationEffect::Relocate(right)) => {
            if let RelocateState::Applied {
                source_after,
                destination_after,
            } = right.state()
                && (!matches!(source_after, DurableObservation::Absent)
                    || !matches!(destination_after, DurableObservation::File { .. }))
            {
                return Err(invalid());
            }
        }
        (OperationEffect::CreateDirectory(left), OperationEffect::CreateDirectory(right)) => {
            use CreateDirectoryState::*;
            match (left.state(), right.state()) {
                (StageIntent, Staged { stage })
                    if !matches!(stage, DurableObservation::Directory { .. }) =>
                {
                    return Err(invalid());
                }
                (Staged { stage: before }, PublishIntent { stage: after }) if before != after => {
                    return Err(invalid());
                }
                (PublishIntent { stage }, Applied { live })
                | (Applied { live }, DiscardIntent { directory: stage })
                    if live != stage =>
                {
                    return Err(invalid());
                }
                (DiscardIntent { directory }, PostimageDiscarded { discard })
                    if directory != discard =>
                {
                    return Err(invalid());
                }
                _ => {}
            }
        }
        (
            OperationEffect::PostCommitRemoveDirectory(left),
            OperationEffect::PostCommitRemoveDirectory(right),
        ) => {
            use RemoveDirectoryState::*;
            match (left.state(), right.state()) {
                (
                    Planned { directory: before },
                    RemoveIntent {
                        directory: after,
                        live,
                    },
                )
                | (
                    RemoveIntent {
                        directory: before,
                        live,
                    },
                    Applied { directory: after },
                ) if before != after
                    || (!matches!(live, DurableObservation::Absent) && live != before) =>
                {
                    return Err(invalid());
                }
                _ => {}
            }
        }
        _ => return Err(invalid()),
    }
    Ok(())
}
