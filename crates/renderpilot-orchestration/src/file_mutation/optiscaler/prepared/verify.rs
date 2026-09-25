use std::path::Path;

use super::super::{
    AppliedOperation, DomainOperationEffect, DomainVerifyState, OptiScalerAction, durable,
    endpoint_for, set_endpoint_expected, token_drift,
};
use super::PreparedFileMutation;
use crate::ServiceError;

impl PreparedFileMutation<'_> {
    pub(crate) fn verify_unchanged(
        &mut self,
        path: &Path,
    ) -> Result<AppliedOperation, ServiceError> {
        let index = self.current_operation_index(path, OptiScalerAction::Verify)?;
        let before = {
            let endpoint = endpoint_for(self.journal.operations()[index].effect(), path)
                .ok_or_else(|| crate::failed("verify endpoint is not current"))?;
            self.resolve_before(index, endpoint)?
        };
        let actual = self.observe_endpoint(path)?;
        if actual != before {
            return Err(token_drift(path, &before, &actual));
        }
        let mut next = self.journal.clone();
        if let DomainOperationEffect::Verify(effect) = next.operations_mut()[index].effect_mut() {
            *effect.state_mut() = DomainVerifyState::Applied {
                observed: durable(&actual),
            };
            set_endpoint_expected(effect.endpoint_mut(), &actual);
        }
        self.cas(next)?;
        self.next_operation_id += 1;
        let ordinal =
            u32::try_from(index).map_err(|_| crate::failed("operation index overflow"))?;
        Ok(AppliedOperation { ordinal })
    }
}
