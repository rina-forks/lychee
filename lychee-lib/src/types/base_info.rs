use reqwest::Url;
use std::path::Path;

use crate::Base;
use crate::ErrorKind;
use crate::InputSource;
use crate::Uri;
use crate::types::uri::raw::RawUri;
use crate::utils::url::ReqwestUrlExt;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SourceBaseInfo {
    /// Tuple of `origin`, `subpath`, `allow_absolute`
    base: Option<(Url, String, bool)>,
    remote_local_mappings: Vec<(Url, Url)>,
}

impl SourceBaseInfo {
    pub fn new(
        base: Option<(Url, String, bool)>,
        remote_local_mappings: Vec<(Url, Url)>,
    ) -> Result<SourceBaseInfo, ErrorKind> {
        let conflicting_mapping = remote_local_mappings.iter().find(|(remote, local)| {
            if remote == local {
                false
            } else {
                remote.strip_prefix(local).is_some() || local.strip_prefix(remote).is_some()
            }
        });

        match conflicting_mapping {
            Some((base, root)) => Err(ErrorKind::InvalidBase(
                base.to_string(),
                format!("base is parent or child of {root}"),
            )),
            None => Ok(Self {
                base,
                remote_local_mappings,
            }),
        }
    }

    fn infer_default_base(url: &Url) -> Result<(Url, String, bool), ErrorKind> {
        let origin = url
            .join("/")
            .map_err(|e| ErrorKind::ParseUrl(e, url.to_string()))?;
        let subpath = origin
            .make_relative(url)
            .expect("failed make a url relative to its own origin root?!");
        Ok((origin, subpath, url.scheme() != "file"))
    }

    pub fn from_source(
        source: &InputSource,
        root_dir: Option<&Path>,
        base: Option<&Base>,
    ) -> Result<SourceBaseInfo, ErrorKind> {
        let root_dir_url = root_dir
            .map(|path| Base::Local(path.to_owned()).to_url())
            .transpose()?;

        // println!("{:?}", base.clone());
        let base_url: Option<Url> = base
            .map(Base::to_url)
            .transpose()?
            .or_else(|| root_dir_url.clone());

        let source_url = source.to_url()?;

        let remote_local_mappings = match (base_url, root_dir_url) {
            (Some(base_url), Some(root_dir_url)) => vec![(base_url, root_dir_url)],
            _ => vec![],
        };

        let Some(source_url) = source_url else {
            return Self::new(None, remote_local_mappings);
        };

        let base = remote_local_mappings
            .iter()
            .find_map(|(remote, local)| {
                source_url
                    .strip_prefix(local)
                    .map(|subpath| (remote.clone(), subpath, true))
            })
            .map_or_else(|| SourceBaseInfo::infer_default_base(&source_url), Ok)?;

        Self::new(Some(base), remote_local_mappings)
    }

    pub fn parse_uri(&self, raw_uri: &RawUri) -> Result<Uri, ErrorKind> {
        let is_absolute = || raw_uri.text.trim_ascii_start().starts_with('/');

        let Uri { url } = Uri::try_from(raw_uri.clone()).or_else(|e| match &self.base {
            Some((_, _, _allow_absolute @ false)) if is_absolute() => {
                Err(ErrorKind::InvalidBaseJoin(raw_uri.text.clone()))
            }
            Some((origin, subpath, _)) => origin
                .join_rooted(&[subpath, &raw_uri.text])
                .map_err(|e| ErrorKind::ParseUrl(e, raw_uri.text.clone()))
                .map(|url| Uri { url }),
            None => Err(e),
        })?;

        let url = self
            .remote_local_mappings
            .iter()
            .find_map(|(remote, local)| {
                url.strip_prefix(remote)
                    .and_then(|subpath| local.join(&subpath).ok())
            })
            .unwrap_or(url);

        Ok(Uri { url })
    }
}
