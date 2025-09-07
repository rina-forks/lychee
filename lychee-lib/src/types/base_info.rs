use reqwest::Url;
use std::path::Path;

use crate::Base;
use crate::ErrorKind;
use crate::InputSource;
use crate::Uri;
use crate::types::uri::raw::RawUri;
use crate::utils::url::ReqwestUrlExt;
use url::PathSegmentsMut;

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
        // TODO: check no repeated bases/roots on the same side.
        // TODO: choose longest match if multiple could apply
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
                format!("base cannot be parent or child of root-dir {root}"),
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
        root_and_base: Option<(&Path, Option<&Base>)>,
        fallback_base: Option<&Base>,
    ) -> Result<SourceBaseInfo, ErrorKind> {
        let root_and_base: Option<(Url, Url)> = match root_and_base {
            Some((root, Some(base))) => Some((root, base.clone())),
            Some((root, None)) => Some((root, Base::Local(root.to_owned()))),
            None => None,
        }
        .map(|(root, base)| -> Result<_, ErrorKind> {
            let root_url = Base::Local(root.to_owned()).to_url()?;
            Ok((root_url, base.to_url()?))
        })
        .transpose()?;

        let source_url = source.to_url()?;

        let remote_local_mappings = match root_and_base {
            Some((root_dir_url, base_url)) => vec![(base_url, root_dir_url)],
            _ => vec![],
        };

        let fallback_base_url = fallback_base.map(Base::to_url).transpose()?;
        let fallback_base_option =
            move || fallback_base_url.map(|url| (url.clone(), String::new(), true));

        let Some(source_url) = source_url else {
            return Self::new(fallback_base_option(), remote_local_mappings);
        };

        let base = remote_local_mappings
            .iter()
            .find_map(|(remote, local)| {
                source_url
                    .strip_prefix(local)
                    .map(|subpath| (remote.clone(), subpath, true))
            })
            .map_or_else(
                || match Self::infer_default_base(&source_url) {
                    ok @ Ok((_, _, _allow_absolute @ false)) => {
                        fallback_base_option().map_or(ok, Ok)
                    }
                    Ok(x) => Ok(x),
                    Err(e) => fallback_base_option().ok_or(e),
                },
                Ok,
            )?;

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

        // println!("before mappings: {}", url.as_str());

        let mut url = self
            .remote_local_mappings
            .iter()
            .find_map(|(remote, local)| {
                url.strip_prefix(remote)
                    .and_then(|subpath| local.join(&subpath).ok())
            })
            .unwrap_or(url);

        // BACKWARDS COMPAT: delete trailing slash for file urls
        if url.scheme() == "file" {
            let _ = url
                .path_segments_mut()
                .as_mut()
                .map(PathSegmentsMut::pop_if_empty);
        }

        Ok(Uri { url })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_base_with_filename() {
        let root_dir = PathBuf::from("/some");
        let base = Base::try_from("https://example.com/path/page2.html").unwrap();
        let source = InputSource::FsPath(PathBuf::from("/some/page.html"));
        let base_info =
            SourceBaseInfo::from_source(&source, Some((&root_dir, Some(&base))), None).unwrap();

        assert_eq!(
            base_info
                .parse_uri(&RawUri::from("#fragment"))
                .as_ref()
                .map(|x| x.url.as_str()),
            Ok("file:///some/page.html#fragment")
        );
    }

    #[test]
    fn test_base_with_same_filename() {
        let root_dir = PathBuf::from("/some/pagex.html");
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = InputSource::FsPath(PathBuf::from("/some/pagex.html"));
        let base_info =
            SourceBaseInfo::from_source(&source, Some((&root_dir, Some(&base))), None).unwrap();

        assert_eq!(
            base_info
                .parse_uri(&RawUri::from("#fragment"))
                .as_ref()
                .map(|x| x.url.as_str()),
            Ok("file:///some/pagex.html#fragment")
        );
    }
}
