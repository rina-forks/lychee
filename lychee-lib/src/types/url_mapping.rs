use reqwest::Url;
use crate::ErrorKind;
use crate::utils::url::ReqwestUrlExt;

pub struct UrlMappings {
    /// List of tuples of `old_url`, `new_url`.
    mappings: Vec<(Url, Url)>,
}

impl UrlMappings {
    pub fn new(mappings: Vec<(Url, Url)>) -> Result<Self, ErrorKind> {
        // TODO: check no repeated bases/roots on the same side.
        // TODO: choose longest match if multiple could apply
        let conflicting_mapping = mappings.iter().find(|(remote, local)| {
            if remote == local {
                false
            } else {
                remote.strip_prefix(local).is_some() || local.strip_prefix(remote).is_some()
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

    pub fn map_to_old_url(&self, url: &Url) -> Option<(&Url, String)> {
        self.mappings
            .iter()
            .find_map(|(left, right)| url.strip_prefix(left).map(|subpath| (right, subpath)))
    }

    pub fn map_to_new_url(&self, url: &Url) -> Option<(&Url, String)> {
        self.mappings
            .iter()
            .find_map(|(left, right)| url.strip_prefix(right).map(|subpath| (left, subpath)))
    }
}

