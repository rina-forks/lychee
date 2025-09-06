use reqwest::Url;
use std::path::Path;

use crate::Base;
use crate::ErrorKind;
use crate::InputSource;
use crate::Uri;
use crate::types::uri::raw::RawUri;
use crate::utils::reqwest::ReqwestUrlExt;
use crate::utils::url::apply_rooted_base_url;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SourceBaseInfo {
    origin: Url,
    subpath: String,
    allow_absolute: bool,
    remote_local_mappings: Vec<(Url, Url)>,
}

impl SourceBaseInfo {
    fn infer_source_base(url: &Url) -> Option<(Url, String, bool)> {
        let origin = url.join("/").ok()?;
        let subpath = origin.make_relative(url)?;
        Some((origin, subpath, url.scheme() != "file"))
    }

    pub fn from_source(
        source: &InputSource,
        root_dir: Option<&Path>,
        base: Option<&Base>,
    ) -> Result<Option<SourceBaseInfo>, ErrorKind> {
        let root_dir_url = root_dir
            .map(|path| Base::Local(path.to_owned()).to_url())
            .transpose()?;

        println!("{:?}", base.clone());
        let base_url: Option<Url> = base
            .map(Base::to_url)
            .transpose()?
            .or_else(|| root_dir_url.clone());

        let source_url = source.to_url()?;

        let Some(source_url) = source_url else {
            return Ok(None);
        };

        let remote_local_mappings = match (base_url, root_dir_url) {
            (Some(base_url), Some(root_dir_url)) => vec![(base_url, root_dir_url)],
            _ => vec![],
        };

        let (origin, subpath, allow_absolute) = remote_local_mappings
            .iter()
            .find_map(|(remote, local)| {
                source_url
                    .strip_prefix(local)
                    .map(|subpath| (remote.clone(), subpath, true))
            })
            .map_or_else(
                || SourceBaseInfo::infer_source_base(&source_url).ok_or(ErrorKind::InvalidUrlHost),
                Ok,
            )?;

        Ok(Some(Self {
            origin,
            subpath,
            allow_absolute,
            remote_local_mappings,
        }))
    }

    pub fn parse_uri(&self, raw_uri: &RawUri) -> Result<Uri, ErrorKind> {
        let Self {
            origin,
            subpath,
            allow_absolute,
            remote_local_mappings,
        } = self;

        let is_absolute = raw_uri.text.trim_ascii_start().starts_with('/');
        if !allow_absolute && is_absolute {
            return Err(ErrorKind::InvalidBaseJoin(raw_uri.text.clone()));
        }

        match apply_rooted_base_url(origin, &[subpath, &raw_uri.text]) {
            Ok(url) => remote_local_mappings
                .iter()
                .find_map(|(remote, local)| {
                    url.strip_prefix(remote)
                        .and_then(|subpath| local.join(&subpath).ok())
                })
                .map_or(Ok(url), Ok),
            Err(e) => Err(e),
        }
        .map_err(|e| ErrorKind::ParseUrl(e, raw_uri.text.clone()))
        .map(|url| Uri { url })
    }
}
