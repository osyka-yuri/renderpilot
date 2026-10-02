//! Finite whole-file ownership claims supported by ordinary RenoDX receipts.

use std::{collections::BTreeMap, path::Path};

use renderpilot_domain::{
    AddonKind, InstalledAddon, InstalledAddonHostKind, PathRef, Sha256Hash, TrackedSource,
    TrackedSourceRole, normalized_path_key,
};
use sha2::{Digest, Sha256};

use crate::{
    addons::renodx::types::renodx_ini_defaults, addons::reshade::ini_schema::ini_merge_strategy,
};

use super::{LocalCleanupCategory, LocalCleanupClaim, insert_if_below_root};

pub(super) fn insert_claims(
    claims: &mut BTreeMap<String, LocalCleanupClaim>,
    root: &PathRef,
    addon: &InstalledAddon,
) {
    if addon.kind() != AddonKind::RenoDx {
        return;
    }

    let created = addon
        .created_files()
        .iter()
        .map(|path| (normalized_path_key(path.as_str()), path))
        .collect::<BTreeMap<_, _>>();
    let addon_key = normalized_path_key(addon.addon_file().as_str());

    if created.contains_key(&addon_key)
        && let Some(digest) =
            authoritative_digest(addon.tracked_sources(), TrackedSourceRole::AddonPayload)
    {
        insert_owned_file_claim(claims, root, addon.addon_file().clone(), digest);
    }

    if addon
        .host_kind()
        .is_none_or(|kind| kind == InstalledAddonHostKind::Proxy)
    {
        let mut host_candidate = None;
        let mut ambiguous_host = false;
        for (key, path) in &created {
            if key != &addon_key
                && Path::new(path.as_str())
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(crate::addons::reshade::scan::is_proxy_slot)
            {
                if host_candidate.is_some() {
                    ambiguous_host = true;
                    break;
                }
                host_candidate = Some(*path);
            }
        }

        if !ambiguous_host
            && let (Some(path), Some(digest)) = (
                host_candidate,
                authoritative_digest(addon.tracked_sources(), TrackedSourceRole::HostBinary),
            )
        {
            insert_owned_file_claim(claims, root, path.clone(), digest);
        }
    }

    if addon.renodx_config_receipt().is_none() {
        let defaults = renodx_ini_defaults();
        let generated = ini_merge_strategy(&defaults).apply("");
        let digest = Sha256Hash::new(hex::encode(Sha256::digest(generated.as_bytes())))
            .expect("SHA-256 output has a valid typed digest");
        for path in created.values().filter(|path| {
            Path::new(path.as_str())
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.eq_ignore_ascii_case(crate::addons::reshade::scan::RESHADE_INI_FILE_NAME)
                })
        }) {
            insert_owned_file_claim(claims, root, (*path).clone(), digest.clone());
        }
    }
}

fn authoritative_digest(sources: &[TrackedSource], role: TrackedSourceRole) -> Option<Sha256Hash> {
    let mut digest: Option<Sha256Hash> = None;

    for source in sources
        .iter()
        .filter(|source| source.role() == role && !source.is_advisory())
    {
        let candidate = Sha256Hash::new(source.digest()).ok()?;
        if digest.as_ref().is_some_and(|known| known != &candidate) {
            return None;
        }
        digest = Some(candidate);
    }

    digest
}

fn insert_owned_file_claim(
    claims: &mut BTreeMap<String, LocalCleanupClaim>,
    root: &PathRef,
    path: PathRef,
    digest: Sha256Hash,
) {
    insert_if_below_root(
        claims,
        root,
        LocalCleanupClaim {
            path,
            category: LocalCleanupCategory::AddonFile,
            sha256: Some(digest),
            native_identity: None,
            removable: true,
            blocked_reason: None,
        },
    );
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use renderpilot_domain::{
        GameId, InstalledAddon, RenoDxConfigReceipt, RenoDxSetPathBaseline, RenoDxSetPathValue,
        TrackedSource,
    };

    use super::*;

    fn path(value: &str) -> PathRef {
        PathRef::new(value).expect("path")
    }

    fn digest(byte: char) -> Sha256Hash {
        Sha256Hash::new(byte.to_string().repeat(Sha256Hash::HEX_LENGTH)).expect("digest")
    }

    fn source(role: TrackedSourceRole, digest: &Sha256Hash) -> TrackedSource {
        TrackedSource::new(
            role,
            "https://example.invalid/source",
            None,
            digest.as_str(),
        )
    }

    fn record() -> InstalledAddon {
        let addon = path("C:/Games/Example/renodx-example.addon64");
        InstalledAddon::new(
            GameId::new("manual:renodx-claims").expect("game id"),
            AddonKind::RenoDx,
            addon,
        )
        .with_created_file(path("C:/Games/Example/dxgi.dll"))
        .with_created_file(path("C:/Games/Example/ReShade.ini"))
        .with_host_kind(InstalledAddonHostKind::Proxy)
        .with_tracked_sources(vec![
            source(TrackedSourceRole::AddonPayload, &digest('a')),
            source(TrackedSourceRole::HostBinary, &digest('b')),
        ])
    }

    fn claims(addon: &InstalledAddon) -> BTreeMap<String, LocalCleanupClaim> {
        let mut claims = BTreeMap::new();
        insert_claims(&mut claims, &path("C:/Games/Example"), addon);
        claims
    }

    #[test]
    fn normal_receipt_claims_exact_payload_proxy_and_default_ini() {
        let addon = record();
        let claims = claims(&addon);

        assert_eq!(claims.len(), 3);
        let payload = claims
            .get(&normalized_path_key(addon.addon_file().as_str()))
            .expect("payload claim");
        assert_eq!(payload.sha256(), Some(&digest('a')));
        assert_eq!(payload.category(), LocalCleanupCategory::AddonFile);
        assert!(payload.removable());
        assert!(payload.native_identity().is_none());

        let host = claims
            .get(&normalized_path_key("C:/Games/Example/dxgi.dll"))
            .expect("proxy claim");
        assert_eq!(host.sha256(), Some(&digest('b')));

        let generated_ini = ini_merge_strategy(&renodx_ini_defaults()).apply("");
        let expected_ini = Sha256Hash::new(hex::encode(Sha256::digest(generated_ini.as_bytes())))
            .expect("generated default hash");
        let ini = claims
            .get(&normalized_path_key("C:/Games/Example/ReShade.ini"))
            .expect("default config claim");
        assert_eq!(ini.sha256(), Some(&expected_ini));
    }

    #[test]
    fn normal_receipt_advisory_malformed_and_conflicting_source_digests_fail_closed_by_role() {
        let addon = record().with_tracked_sources(vec![
            source(TrackedSourceRole::AddonPayload, &digest('a')).with_advisory(),
            TrackedSource::new(
                TrackedSourceRole::AddonPayload,
                "https://example.invalid/bad",
                None,
                "malformed",
            ),
            source(TrackedSourceRole::HostBinary, &digest('b')),
            source(TrackedSourceRole::HostBinary, &digest('c')),
        ]);

        let conflicting_claims = claims(&addon);
        assert!(
            !conflicting_claims.contains_key(&normalized_path_key(addon.addon_file().as_str()))
        );
        assert!(
            !conflicting_claims.contains_key(&normalized_path_key("C:/Games/Example/dxgi.dll"))
        );
        assert!(
            conflicting_claims.contains_key(&normalized_path_key("C:/Games/Example/ReShade.ini"))
        );

        let cases = [
            (
                vec![
                    source(TrackedSourceRole::AddonPayload, &digest('a')).with_advisory(),
                    source(TrackedSourceRole::HostBinary, &digest('b')),
                ],
                false,
                true,
            ),
            (
                vec![
                    source(TrackedSourceRole::AddonPayload, &digest('a')),
                    source(TrackedSourceRole::HostBinary, &digest('b')).with_advisory(),
                ],
                true,
                false,
            ),
            (
                vec![source(TrackedSourceRole::HostBinary, &digest('b'))],
                false,
                true,
            ),
            (
                vec![source(TrackedSourceRole::AddonPayload, &digest('a'))],
                true,
                false,
            ),
        ];
        for (sources, payload_claimed, host_claimed) in cases {
            let candidate = record().with_tracked_sources(sources);
            let claims = claims(&candidate);
            assert_eq!(
                claims.contains_key(&normalized_path_key(candidate.addon_file().as_str())),
                payload_claimed
            );
            assert_eq!(
                claims.contains_key(&normalized_path_key("C:/Games/Example/dxgi.dll")),
                host_claimed
            );
        }
    }

    #[test]
    fn normal_receipt_identical_valid_sources_are_allowed_but_ambiguous_proxy_bindings_are_not() {
        let addon = record()
            .with_created_file(path("C:/Games/Example/d3d11.dll"))
            .with_tracked_sources(vec![
                source(TrackedSourceRole::AddonPayload, &digest('a')),
                source(TrackedSourceRole::AddonPayload, &digest('a')),
                source(TrackedSourceRole::HostBinary, &digest('b')),
            ]);

        let claims = claims(&addon);
        assert!(claims.contains_key(&normalized_path_key(addon.addon_file().as_str())));
        assert!(!claims.contains_key(&normalized_path_key("C:/Games/Example/dxgi.dll")));
        assert!(!claims.contains_key(&normalized_path_key("C:/Games/Example/d3d11.dll")));
    }

    #[test]
    fn normal_receipt_default_ini_route_requires_unreceipted_canonical_owned_path() {
        let with_receipt = record()
            .with_renodx_config_receipt(Some(RenoDxConfigReceipt::new(
                path("C:/Games/Example/ReShade.ini"),
                RenoDxSetPathBaseline::Absent,
                false,
                RenoDxSetPathValue::Zero,
            )))
            .expect("valid typed config receipt");
        let receipt_claims = claims(&with_receipt);
        assert!(!receipt_claims.contains_key(&normalized_path_key("C:/Games/Example/ReShade.ini")));

        let unowned_ini = InstalledAddon::new(
            GameId::new("manual:renodx-claims-unowned-ini").expect("game id"),
            AddonKind::RenoDx,
            path("C:/Games/Example/renodx-example.addon64"),
        )
        .with_created_file(path("C:/Games/Example/dxgi.dll"))
        .with_host_kind(InstalledAddonHostKind::Proxy)
        .with_tracked_sources(vec![
            source(TrackedSourceRole::AddonPayload, &digest('a')),
            source(TrackedSourceRole::HostBinary, &digest('b')),
        ]);
        assert!(
            !claims(&unowned_ini)
                .contains_key(&normalized_path_key("C:/Games/Example/ReShade.ini"))
        );

        let adopted = crate::addons::record::adopt_existing_paths(
            unowned_ini,
            &[std::path::PathBuf::from("C:/Games/Example/ReShade.ini")],
        )
        .expect("adopt the existing canonical path");
        assert!(
            claims(&adopted).contains_key(&normalized_path_key("C:/Games/Example/ReShade.ini"))
        );
    }

    #[test]
    fn normal_receipt_shared_vulkan_host_and_other_addon_kinds_do_not_gain_host_claims() {
        let shared = record().with_host_kind(InstalledAddonHostKind::SharedVulkanLayer);
        let shared_claims = claims(&shared);
        assert!(!shared_claims.contains_key(&normalized_path_key("C:/Games/Example/dxgi.dll")));

        let other_kind = InstalledAddon::new(
            GameId::new("manual:luma-claims").expect("game id"),
            AddonKind::Luma,
            path("C:/Games/Example/luma.addon"),
        )
        .with_created_file(path("C:/Games/Example/dxgi.dll"))
        .with_tracked_source(source(TrackedSourceRole::AddonPayload, &digest('a')));
        assert!(claims(&other_kind).is_empty());
    }
}
