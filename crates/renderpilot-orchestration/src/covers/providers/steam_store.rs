//! Resolves regional or retired Steam package IDs through Steam's own store search.

use reqwest::{Url, blocking::Client};
use serde::Deserialize;

use super::{download::download_unvalidated_cover, steam_cdn};
use crate::ServiceError;

const STEAM_STORE_SEARCH_URL: &str = "https://store.steampowered.com/api/storesearch/";
const REGIONAL_TITLE_SUFFIXES: &[&str] = &[" (pcr)", " pcr"];

#[derive(Debug, Deserialize)]
struct StoreSearchResponse {
    #[serde(default)]
    items: Vec<StoreSearchItem>,
}

#[derive(Debug, Deserialize)]
struct StoreSearchItem {
    id: u64,
    name: String,
}

pub(super) fn try_canonical_steam_cover(
    client: &Client,
    installed_app_id: &str,
    installed_title: &str,
) -> Result<Vec<u8>, ServiceError> {
    let search_title = canonicalize_regional_title(installed_title);
    let url = store_search_url(search_title)?;
    let response = download_unvalidated_cover(client, url.as_str())?;
    let search = serde_json::from_slice::<StoreSearchResponse>(&response)
        .map_err(|error| ServiceError::CoverDownloadFailed(error.to_string()))?;

    let app_id = unique_canonical_app_id(&search.items, installed_app_id, search_title)
        .ok_or(ServiceError::CoverNotFound)?;

    steam_cdn::try_steam_cdn(client, &app_id.to_string())
}

fn store_search_url(title: &str) -> Result<Url, ServiceError> {
    let mut url = Url::parse(STEAM_STORE_SEARCH_URL)
        .map_err(|error| ServiceError::CoverDownloadFailed(error.to_string()))?;
    url.query_pairs_mut()
        .append_pair("term", title)
        .append_pair("l", "english")
        .append_pair("cc", "US");
    Ok(url)
}

fn canonicalize_regional_title(title: &str) -> &str {
    let title = title.trim();

    REGIONAL_TITLE_SUFFIXES
        .iter()
        .find_map(|suffix| strip_ascii_case_insensitive_suffix(title, suffix))
        .map(str::trim_end)
        .unwrap_or(title)
}

fn strip_ascii_case_insensitive_suffix<'a>(value: &'a str, suffix: &str) -> Option<&'a str> {
    let prefix_length = value.len().checked_sub(suffix.len())?;
    let prefix = value.get(..prefix_length)?;
    let candidate_suffix = value.get(prefix_length..)?;

    candidate_suffix
        .eq_ignore_ascii_case(suffix)
        .then_some(prefix)
}

fn unique_canonical_app_id(
    items: &[StoreSearchItem],
    installed_app_id: &str,
    title: &str,
) -> Option<u64> {
    let expected_title = normalized_title_key(title);
    let installed_app_id = installed_app_id.trim().parse::<u64>().ok();
    let mut matches = items
        .iter()
        .filter(|item| Some(item.id) != installed_app_id)
        .filter(|item| normalized_title_key(&item.name) == expected_title)
        .map(|item| item.id);
    let app_id = matches.next()?;

    matches.next().is_none().then_some(app_id)
}

fn normalized_title_key(title: &str) -> String {
    title
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        StoreSearchItem, StoreSearchResponse, canonicalize_regional_title, normalized_title_key,
        store_search_url, unique_canonical_app_id,
    };

    #[test]
    fn strips_known_regional_suffix_without_changing_normal_titles() {
        assert_eq!(
            canonicalize_regional_title("Fallout: New Vegas PCR"),
            "Fallout: New Vegas"
        );
        assert_eq!(
            canonicalize_regional_title("Fallout: New Vegas (PcR)"),
            "Fallout: New Vegas"
        );
        assert_eq!(canonicalize_regional_title("Portal 2"), "Portal 2");
    }

    #[test]
    fn store_search_url_encodes_the_title_as_query_data() -> Result<(), crate::ServiceError> {
        let url = store_search_url("Fallout: New Vegas")?;
        let query = url.query_pairs().collect::<Vec<_>>();

        assert!(query.contains(&("term".into(), "Fallout: New Vegas".into())));
        assert!(query.contains(&("l".into(), "english".into())));
        assert!(query.contains(&("cc".into(), "US".into())));
        Ok(())
    }

    #[test]
    fn accepts_only_exact_normalized_titles_and_skips_the_installed_package_id() {
        let items = vec![
            StoreSearchItem {
                id: 22_490,
                name: String::from("Fallout: New Vegas"),
            },
            StoreSearchItem {
                id: 22_380,
                name: String::from("Fallout New Vegas"),
            },
            StoreSearchItem {
                id: 377_160,
                name: String::from("Fallout 4"),
            },
        ];

        assert_eq!(
            unique_canonical_app_id(&items, "22490", "Fallout: New Vegas"),
            Some(22_380)
        );
    }

    #[test]
    fn rejects_ambiguous_exact_title_matches() {
        let items = vec![
            StoreSearchItem {
                id: 1,
                name: String::from("Prey"),
            },
            StoreSearchItem {
                id: 2,
                name: String::from("Prey"),
            },
        ];

        assert_eq!(unique_canonical_app_id(&items, "3", "Prey"), None);
    }

    #[test]
    fn title_matching_is_case_and_punctuation_insensitive() {
        assert_eq!(
            normalized_title_key("Fallout: New Vegas"),
            normalized_title_key("FALLOUT NEW VEGAS")
        );
    }

    #[test]
    fn parses_the_official_store_search_wire_shape() -> Result<(), serde_json::Error> {
        let response = serde_json::from_str::<StoreSearchResponse>(
            r#"{
                "total": 1,
                "items": [{
                    "type": "app",
                    "name": "Fallout: New Vegas",
                    "id": 22380
                }]
            }"#,
        )?;

        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].id, 22_380);
        Ok(())
    }
}
