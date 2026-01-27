//! Mapping of URLs based on prefix matches of the URL's path structure.
use crate::ErrorKind;
use crate::utils::url::ReqwestUrlExt;
use reqwest::Url;

/// A collection of URL mappings which can be applied in either direction.
///
/// Mappings are from URL to URL. A URL matches with a particular mapping
/// (and hence, the mapping will be applied) when the URL is a subpath
/// of the mapping source URL. Equivalently, this is when the URL has
/// a mapping's source URL as a prefix.
///
/// Mappings are provided as pairs and the mapping can be interpreted in
/// either direction; the left URL can be mapped to the right, or
/// vice-versa.
///
/// Despite this, we call the left side the "old URL" and the right side the
/// "new URL", since most uses will have _some_ level of directionality.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct UrlMappings {
    /// List of tuples of `old_url`, `new_url`.
    mappings: Vec<(Url, Url)>,
}

impl UrlMappings {
    /// Constructs a new [`UrlMappings`] from the given mappings.
    ///
    /// # Errors
    ///
    /// If any pair has a URL which is a subpath of its other URL.
    pub fn new(mappings: Vec<(Url, Url)>) -> Result<Self, ErrorKind> {
        // TODO: check no repeated bases/roots on the same side.
        let conflicting_mapping = mappings.iter().find(|(remote, local)| {
            if remote == local {
                false
            } else {
                remote.strictly_relative_to(local).is_some()
                    || local.strictly_relative_to(remote).is_some()
            }
        });

        match conflicting_mapping {
            Some((base, root)) => Err(ErrorKind::InvalidBase(
                base.to_string(),
                format!("base cannot be parent or child of root-dir {root}"),
            )),
            None => Ok(Self { mappings }),
        }
    }

    /// Matches the given URL against the old (left) URLs and
    /// returns the new (right) URL of the first matched pair, if any.
    ///
    /// If matched, the returned option will contain a URL from the new
    /// side of a mapping, along with the subpath of the given URL when
    /// the corresponding old URL is removed from it.
    pub fn map_to_new_url(&self, url: &Url) -> Option<(&Url, String)> {
        // TODO: choose longest match if multiple could apply??
        self.mappings.iter().find_map(|(left, right)| {
            url.strictly_relative_to(right)
                .map(|subpath| (left, subpath))
        })
    }

    /// Like [`UrlMappings::map_to_new_url`] but in the reverse direction,
    /// matching against the new URLs and returning the correponding
    /// old URL of the matched mapping, if any.
    pub fn map_to_old_url(&self, url: &Url) -> Option<(&Url, String)> {
        self.mappings.iter().find_map(|(left, right)| {
            url.strictly_relative_to(left)
                .map(|subpath| (right, subpath))
        })
    }
}
