//! GOG Galaxy artwork via the public product API and `*.gog-statics.com` vertical grid assets.

use reqwest::{blocking::Client, StatusCode, Url};
use serde::Deserialize;

use super::super::validation::validate_cover_bytes;
use super::download::download_unvalidated_cover;
use crate::error::CliError;

const GOG_API_LOCALE: &str = "en-US";
const GOG_LOGO_SUFFIX: &str = "_glx_logo.jpg";
const GOG_VERTICAL_COVER_SUFFIX: &str = "_glx_vertical_cover.webp";
const GOG_VERTICAL_COVER_QUERY: &str = "namespace=gamesdb";
const GOG_STATICS_HOST_SUFFIX: &str = ".gog-statics.com";

#[derive(Debug, Deserialize)]
struct GogProductImages {
    #[serde(default)]
    logo: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GogProduct {
    #[serde(default)]
    images: Option<GogProductImages>,
}

impl GogProduct {
    fn logo(&self) -> Option<&str> {
        self.images
            .as_ref()
            .and_then(|images| images.logo.as_deref())
            .map(str::trim)
            .filter(|logo| !logo.is_empty())
    }
}

pub(super) fn try_gog_cdn(client: &Client, product_id: &str) -> Result<Vec<u8>, CliError> {
    let product_id = normalize_gog_product_id(product_id)?;
    let product = fetch_gog_product(client, product_id)?;

    let logo = product.logo().ok_or(CliError::CoverNotFound)?;
    let cover_url = gog_vertical_cover_url_from_logo(logo).ok_or(CliError::CoverNotFound)?;

    let bytes = download_unvalidated_cover(client, cover_url.as_str())?;

    validate_cover_bytes(&bytes)
        .map(|_| bytes)
        .map_err(|_| CliError::CoverNotFound)
}

fn normalize_gog_product_id(product_id: &str) -> Result<&str, CliError> {
    let product_id = product_id.trim();

    if product_id.is_empty() || !product_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(CliError::CoverNotFound);
    }

    Ok(product_id)
}

fn fetch_gog_product(client: &Client, product_id: &str) -> Result<GogProduct, CliError> {
    let response = client
        .get(gog_product_meta_url(product_id))
        .send()
        .map_err(download_failed)?;

    let status = response.status();
    if !status.is_success() {
        return Err(map_gog_product_status(status));
    }

    response.json().map_err(download_failed)
}

fn gog_product_meta_url(product_id: &str) -> String {
    format!("https://api.gog.com/products/{product_id}?expand=images&locale={GOG_API_LOCALE}")
}

fn download_failed(error: reqwest::Error) -> CliError {
    CliError::CoverDownloadFailed(error.to_string())
}

fn map_gog_product_status(status: StatusCode) -> CliError {
    match status {
        StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND => CliError::CoverNotFound,
        _ => CliError::CoverDownloadFailed(format!("GOG product API returned HTTP {status}")),
    }
}

/// Builds the Galaxy vertical grid URL from `images.logo` as returned today:
/// `//images-*.gog-statics.com/<sha256>_glx_logo.jpg` →
/// `<sha256>_glx_vertical_cover.webp?namespace=gamesdb`.
///
/// If GOG changes this suffix, update parsing here and the tests below.
fn gog_vertical_cover_url_from_logo(logo: &str) -> Option<String> {
    let mut url = parse_gog_statics_url(logo)?;

    let file_name = url.path_segments()?.next_back()?;
    let base = file_name.strip_suffix(GOG_LOGO_SUFFIX)?;

    if base.is_empty() {
        return None;
    }

    let vertical_file_name = format!("{base}{GOG_VERTICAL_COVER_SUFFIX}");
    let vertical_path = replace_last_path_segment(url.path(), &vertical_file_name);

    url.set_path(&vertical_path);
    url.set_query(Some(GOG_VERTICAL_COVER_QUERY));
    url.set_fragment(None);

    Some(url.to_string())
}

fn parse_gog_statics_url(raw: &str) -> Option<Url> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    let normalized;
    let url_text = if raw.starts_with("//") {
        normalized = format!("https:{raw}");
        normalized.as_str()
    } else {
        raw
    };

    let mut url = Url::parse(url_text).ok()?;

    match url.scheme() {
        "https" => {}
        "http" => {
            url.set_scheme("https").ok()?;
        }
        _ => return None,
    }

    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }

    let host = url.host_str()?.to_ascii_lowercase();
    if !is_gog_statics_host(&host) {
        return None;
    }

    Some(url)
}

fn is_gog_statics_host(host: &str) -> bool {
    host == "gog-statics.com" || host.ends_with(GOG_STATICS_HOST_SUFFIX)
}

fn replace_last_path_segment(path: &str, file_name: &str) -> String {
    match path.rsplit_once('/') {
        Some(("", _)) | None => format!("/{file_name}"),
        Some((prefix, _)) => format!("{prefix}/{file_name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{gog_vertical_cover_url_from_logo, normalize_gog_product_id};

    const HASH: &str = "7f92348005ac8bfeb1eecebd0f41889f011a217668369692f855063e6eada255";

    #[test]
    fn gog_vertical_cover_url_from_typical_logo() {
        let logo = format!("//images-3.gog-statics.com/{HASH}_glx_logo.jpg");

        assert_eq!(
            gog_vertical_cover_url_from_logo(&logo).as_deref(),
            Some(
                "https://images-3.gog-statics.com/7f92348005ac8bfeb1eecebd0f41889f011a217668369692f855063e6eada255_glx_vertical_cover.webp?namespace=gamesdb"
            )
        );
    }

    #[test]
    fn gog_vertical_cover_url_accepts_https_logo() {
        let logo = format!("https://images-3.gog-statics.com/{HASH}_glx_logo.jpg");

        assert_eq!(
            gog_vertical_cover_url_from_logo(&logo).as_deref(),
            Some(
                "https://images-3.gog-statics.com/7f92348005ac8bfeb1eecebd0f41889f011a217668369692f855063e6eada255_glx_vertical_cover.webp?namespace=gamesdb"
            )
        );
    }

    #[test]
    fn gog_vertical_cover_url_upgrades_http_to_https() {
        let logo = format!("http://images-3.gog-statics.com/{HASH}_glx_logo.jpg");

        assert_eq!(
            gog_vertical_cover_url_from_logo(&logo).as_deref(),
            Some(
                "https://images-3.gog-statics.com/7f92348005ac8bfeb1eecebd0f41889f011a217668369692f855063e6eada255_glx_vertical_cover.webp?namespace=gamesdb"
            )
        );
    }

    #[test]
    fn gog_vertical_cover_url_preserves_directory_path() {
        let logo = format!("//images-3.gog-statics.com/assets/{HASH}_glx_logo.jpg");

        assert_eq!(
            gog_vertical_cover_url_from_logo(&logo).as_deref(),
            Some(
                "https://images-3.gog-statics.com/assets/7f92348005ac8bfeb1eecebd0f41889f011a217668369692f855063e6eada255_glx_vertical_cover.webp?namespace=gamesdb"
            )
        );
    }

    #[test]
    fn gog_vertical_cover_url_replaces_existing_query_and_fragment() {
        let logo = format!("//images-3.gog-statics.com/{HASH}_glx_logo.jpg?old=1#logo");

        assert_eq!(
            gog_vertical_cover_url_from_logo(&logo).as_deref(),
            Some(
                "https://images-3.gog-statics.com/7f92348005ac8bfeb1eecebd0f41889f011a217668369692f855063e6eada255_glx_vertical_cover.webp?namespace=gamesdb"
            )
        );
    }

    #[test]
    fn gog_vertical_cover_url_rejects_wrong_suffix() {
        let logo = format!("//images-3.gog-statics.com/{HASH}_logo.jpg");

        assert_eq!(gog_vertical_cover_url_from_logo(&logo), None);
    }

    #[test]
    fn gog_vertical_cover_url_rejects_empty_base() {
        assert_eq!(
            gog_vertical_cover_url_from_logo("//images-3.gog-statics.com/_glx_logo.jpg"),
            None
        );
    }

    #[test]
    fn gog_vertical_cover_url_rejects_non_gog_host() {
        let logo = format!("//example.com/{HASH}_glx_logo.jpg");

        assert_eq!(gog_vertical_cover_url_from_logo(&logo), None);
    }

    #[test]
    fn gog_vertical_cover_url_rejects_credentials() {
        let logo = format!("https://user@images-3.gog-statics.com/{HASH}_glx_logo.jpg");

        assert_eq!(gog_vertical_cover_url_from_logo(&logo), None);
    }

    #[test]
    fn gog_vertical_cover_url_rejects_non_http_scheme() {
        let logo = format!("ftp://images-3.gog-statics.com/{HASH}_glx_logo.jpg");

        assert_eq!(gog_vertical_cover_url_from_logo(&logo), None);
    }

    #[test]
    fn gog_product_id_is_trimmed() {
        assert_eq!(normalize_gog_product_id(" 12345 ").unwrap(), "12345");
    }

    #[test]
    fn gog_product_id_rejects_empty_input() {
        assert!(normalize_gog_product_id("   ").is_err());
    }

    #[test]
    fn gog_product_id_rejects_non_digits() {
        assert!(normalize_gog_product_id("12a45").is_err());
    }
}
