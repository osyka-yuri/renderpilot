fn rollback_verify(
    prepared: &mut PreparedFileMutation<'_>,
    index: usize,
    effect: &DomainVerifyEffect,
) -> Result<(), ServiceError> {
    if matches!(effect.state(), DomainVerifyState::Planned) {
        let mut next = prepared.journal.clone();
        if let DomainOperationEffect::Verify(value) = next.operations_mut()[index].effect_mut() {
            *value.state_mut() = DomainVerifyState::Preserved;
        }
        prepared.cas(next)?;
        return Ok(());
    }
    let path = Path::new(effect.endpoint().path());
    let DomainVerifyState::Applied { observed } = effect.state() else {
        unreachable!();
    };
    let expected = native(observed);
    if observe(path) != expected {
        return Err(token_drift(path, &expected, &observe(path)));
    }
    let mut next = prepared.journal.clone();
    if let DomainOperationEffect::Verify(value) = next.operations_mut()[index].effect_mut() {
        *value.state_mut() = DomainVerifyState::Preserved;
    }
    prepared.cas(next)?;
    Ok(())
}
