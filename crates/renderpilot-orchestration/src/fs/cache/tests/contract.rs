//! Provider-neutral cache contract cases.

#[cfg(not(target_os = "linux"))]
use super::*;

#[cfg(not(target_os = "linux"))]
#[test]
fn cache_contract_retains_published_and_current_results() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("manifest.json");
    let absent = observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes))
        .expect("observe absent cache");
    let published = commit_cache_candidate(
        &path,
        absent.generation(),
        b"published",
        MatchingCurrentPolicy::RefreshCandidate,
        |bytes| parse_doc(bytes).map(|()| bytes.to_vec()),
    )
    .expect("publish absent candidate");
    assert!(matches!(published, CachePublication::Published));

    let observed = observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes))
        .expect("observe published cache");
    fs::write(&path, b"current-winner").expect("install concurrent current winner");
    let result = commit_cache_candidate(
        &path,
        observed.generation(),
        b"losing-candidate",
        MatchingCurrentPolicy::RefreshCandidate,
        |bytes| parse_doc(bytes).map(|()| bytes.to_vec()),
    )
    .expect("classify concurrent current winner");

    assert!(matches!(result, CachePublication::Current(bytes) if bytes == b"current-winner"));
    assert_eq!(
        fs::read(&path).expect("read current winner"),
        b"current-winner"
    );
}
