use std::borrow::Cow;

use renderpilot_domain::{
    NormalizedPathRelation, PeerEndpointRole, RenoDxReshadeIniAuthority, RenoDxReshadeIniFeature,
    normalized_path_relation,
};

use super::effects::{
    ActiveUninstallEffects, emit_remove, emit_replace, image_for_bytes, present_bytes,
    present_image,
};
use super::error::RenoDxActiveUninstallError;
use super::model::ActiveUninstallInput;
use crate::addons::renodx::reshade_ini::{ini_remove_renodx_strategy, plan_config_removal};

pub(super) fn compose_ini(
    input: &ActiveUninstallInput<'_>,
    effects: &mut ActiveUninstallEffects,
) -> Result<Option<RenoDxReshadeIniAuthority>, RenoDxActiveUninstallError> {
    let ini_path = input.root.config_source().exact_ini_path();
    let ini_ref = renderpilot_domain::PathRef::new(
        ini_path
            .to_str()
            .ok_or_else(|| RenoDxActiveUninstallError::Path(ini_path.to_path_buf()))?,
    )
    .map_err(|_| RenoDxActiveUninstallError::Path(ini_path.to_path_buf()))?;
    let created = input.record.created_files().iter().any(|path| {
        matches!(
            normalized_path_relation(path.as_str(), ini_ref.as_str()),
            NormalizedPathRelation::Equal
        )
    });
    let backed = input.record.backed_up_files().iter().any(|path| {
        matches!(
            normalized_path_relation(path.as_str(), ini_ref.as_str()),
            NormalizedPathRelation::Equal
        )
    });
    let named_claim = input
        .record
        .created_files()
        .iter()
        .chain(input.record.backed_up_files())
        .any(|path| {
            path.file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("ReShade.ini"))
                && !matches!(
                    normalized_path_relation(path.as_str(), ini_ref.as_str()),
                    NormalizedPathRelation::Equal
                )
        });
    if named_claim || backed && !created {
        return Err(RenoDxActiveUninstallError::Invalid(
            "ReShade.ini claim is not exact and coherent",
        ));
    }
    let current_bytes = input.ini_snapshot.bytes();
    let should_remove = created && !backed;
    // A whole-file claim retains its established deletion semantics. When the
    // file survives, every receipt-owned key is removed through the typed
    // planner's tolerant per-key CAS. External edits stay in place while
    // unchanged owned keys continue through restoration/removal.
    let transformed = current_bytes
        .map(|bytes| {
            let config_bytes = if let Some(receipt) = input.record.renodx_config_receipt() {
                if !receipt.is_supported()
                    || !matches!(
                        normalized_path_relation(receipt.ini_path.as_str(), ini_ref.as_str()),
                        NormalizedPathRelation::Equal
                    )
                {
                    tracing::warn!(
                        "skipping RenoDX configuration cleanup during active uninstall: receipt is invalid or targets a different ReShade.ini"
                    );
                    Cow::Borrowed(bytes)
                } else {
                    match plan_config_removal(&receipt.ini_path, Some(bytes), receipt) {
                        Ok(plan) => plan
                            .after
                            .map(Cow::Owned)
                            .unwrap_or(Cow::Borrowed(bytes)),
                        Err(error) => {
                            tracing::warn!(
                                "skipping RenoDX configuration cleanup during active uninstall: {error}"
                            );
                            Cow::Borrowed(bytes)
                        }
                    }
                }
            } else {
                Cow::Borrowed(bytes)
            };
            Ok::<Cow<'_, [u8]>, RenoDxActiveUninstallError>(std::str::from_utf8(config_bytes.as_ref())
                .map(|text| Cow::Owned(ini_remove_renodx_strategy().apply(text).into_bytes()))
                .unwrap_or(config_bytes))
        })
        .transpose()?;
    let should_replace = (created && backed)
        || (!created
            && !backed
            && transformed
                .as_ref()
                .is_some_and(|after| current_bytes != Some(after.as_ref())));
    if (created || backed) && current_bytes.is_none() {
        return Err(RenoDxActiveUninstallError::Invalid(
            "claimed ReShade.ini is missing",
        ));
    }
    if !should_remove && !should_replace {
        return Ok(None);
    }
    let authority = RenoDxReshadeIniAuthority::new(
        RenoDxReshadeIniFeature::Uninstall,
        input.root.canonical_game_root_ref().clone(),
    )
    .map_err(|_| RenoDxActiveUninstallError::Invalid("cannot build exact ReShade.ini authority"))?;
    let before_bytes = present_bytes(&ini_ref, input.ini_snapshot)?;
    let before = present_image(&ini_ref, input.ini_snapshot)?;
    if should_remove {
        emit_remove(
            &ini_ref,
            PeerEndpointRole::RenoDxReshadeIni,
            before,
            before_bytes,
            effects,
        );
    } else {
        let after = transformed
            .ok_or(RenoDxActiveUninstallError::Invalid(
                "ReShade.ini transform has no bytes",
            ))?
            .into_owned();
        let after_image = image_for_bytes(&after)?;
        emit_replace(
            &ini_ref,
            PeerEndpointRole::RenoDxReshadeIni,
            before,
            before_bytes,
            &after_image,
            after,
            effects,
        );
    }
    Ok(Some(authority))
}
