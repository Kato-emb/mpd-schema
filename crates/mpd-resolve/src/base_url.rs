//! Hierarchical `BaseURL` resolution down the MPD spine.

use mpd_schema::model::BaseUrl;
use url::Url;

use crate::error::{Error, ErrorKind};
use crate::segment::CandidateUrl;

/// Resolves one level's `BaseURL` children against the candidates inherited
/// from the enclosing levels.
///
/// With no `BaseURL` at this level the parent candidates pass through
/// unchanged. With one or more, each parent candidate is the base for each
/// `BaseURL` at this level (the cross product), and the resulting candidate
/// takes this level's `serviceLocation` because it is the deepest `BaseURL`
/// that contributed it.
pub(crate) fn resolve_level(
    parents: &[CandidateUrl],
    base_urls: &[BaseUrl],
    path: &str,
) -> Result<Vec<CandidateUrl>, Error> {
    if base_urls.is_empty() {
        return Ok(parents.to_vec());
    }
    let mut resolved = Vec::new();
    for parent in parents {
        for base_url in base_urls {
            let joined = parent.url.join(&base_url.url).map_err(|_| {
                Error::new(
                    path.to_string(),
                    ErrorKind::InvalidBaseUrl {
                        value: base_url.url.clone(),
                    },
                )
            })?;
            resolved.push(CandidateUrl::new(joined, base_url.service_location.clone()));
        }
    }
    Ok(resolved)
}

/// Parses the absolute manifest base URL supplied by the caller.
///
/// # Errors
///
/// Returns [`ErrorKind::InvalidBaseUrl`] when `base` is not an absolute URL.
pub(crate) fn parse_manifest_base(base: &str) -> Result<Url, Error> {
    Url::parse(base).map_err(|_| {
        Error::new(
            String::new(),
            ErrorKind::InvalidBaseUrl {
                value: base.to_string(),
            },
        )
    })
}
