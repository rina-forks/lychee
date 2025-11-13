use reqwest::Url;
use std::path::Path;

use crate::Base;
use crate::ErrorKind;
use crate::ResolvedInputSource;
use crate::Uri;
use crate::types::uri::raw::RawUri;
use crate::utils::url::ReqwestUrlExt;
use url::PathSegmentsMut;

/// Information needed for resolving relative URLs within a particular
/// [`InputSource`]. The main entry point for constructing a `SourceBaseInfo`
/// is [`SourceBaseInfo::from_source`]. Once constructed,
/// [`SourceBaseInfo::parse_uri`] can be used to parse a URI found within
/// the `InputSource`.
///
/// A `SourceBaseInfo` may or may not have an associated base which is used
/// for resolving relative URLs. If no base is available, parsing relative
/// and root-relative links will fail. If a base is available but it is not
/// *well-founded*, then parsing root-relative links will fail. See
/// [`SourceBaseInfo::from_source`] for a description of well-founded.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SourceBaseInfo {
    /// Tuple of `origin`, `subpath`, `allow_absolute`. The field `allow_absolute`
    /// is true if the base is well-founded.
    base: Option<(Url, String, bool)>,
    /// List of tuples of `remote_url`, `local_url`.
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

    /// Constructs a `SourceBaseInfo` from the given input source, root and base
    /// pair, and fallback base.
    ///
    /// # Arguments
    ///
    /// * `source` - The input source which contains the links we want to resolve.
    /// * `root_and_base` - An optional pair of root directory and base URL. The
    ///   somewhat complicated type encodes the fact that if a [`Base`] is provided,
    ///   then a [`Path`] must be provided too. If the base URL is omitted but root
    ///   dir is provided, the base URL defaults to the root dir.
    /// * `fallback_base` - A fallback base URL to use where no other well-founded
    ///   base URL can be derived. If it is applied, the fallback base URL is
    ///   considered to be a well-founded base.
    ///
    /// # Root and base
    ///
    /// The given root and base URL are used to transform the intrinsic base returned
    /// by [`InputSource::to_url`]. If the intrinsic base is a subpath of the given
    /// root, then a new base is constructed by taking the intrinsic base and replacing
    /// the root dir with the given base URL.
    ///
    /// In this way, links from local files can be resolved *as if* they were hosted
    /// in a remote location at the base URL. Later, in [`SourceBaseInfo::parse_uri`],
    /// remote links which are subpaths of the base URL will be reflected back to
    /// local files within the root dir.
    ///
    /// # Well-founded bases
    ///
    /// Formally, a *well-founded* base is one which is derived from an input
    /// source which is *not* a local file, or one derived from a local file
    /// source which is a descendent of the given root dir.
    ///
    /// Informally, and importantly for using [`SourceBaseInfo`], a well-founded
    /// base is one where we can sensibly resolve root-relative links (i.e.,
    /// relative links starting with `/`).
    ///
    /// # Errors
    ///
    /// This function fails with an [`Err`] if:
    /// - any of the provided arguments cannot be converted to a URL, or
    /// - [`SourceBaseInfo::new`] fails.
    pub fn from_source(
        source: &ResolvedInputSource,
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
    use std::num::NonZeroUsize;
    use std::path::PathBuf;

    use crate::types::uri::raw::RawUriSpan;

    fn raw_uri(text: &'static str) -> RawUri {
        RawUri {
            text: text.to_string(),
            element: None,
            attribute: None,
            span: RawUriSpan {
                line: NonZeroUsize::MAX,
                column: None,
            },
        }
    }

    #[test]
    fn test_base_with_filename() {
        let root_dir = PathBuf::from("/some");
        let base = Base::try_from("https://example.com/path/page2.html").unwrap();
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));
        let base_info =
            SourceBaseInfo::from_source(&source, Some((&root_dir, Some(&base))), None).unwrap();

        assert_eq!(
            base_info
                .parse_uri(&raw_uri("#fragment"))
                .as_ref()
                .map(|x| x.url.as_str()),
            Ok("file:///some/page.html#fragment")
        );
    }

    #[test]
    fn test_base_with_same_filename() {
        let root_dir = PathBuf::from("/some/pagex.html");
        let base = Base::try_from("https://example.com/path/page.html").unwrap();
        let source = ResolvedInputSource::FsPath(PathBuf::from("/some/pagex.html"));
        let base_info =
            SourceBaseInfo::from_source(&source, Some((&root_dir, Some(&base))), None).unwrap();

        assert_eq!(
            base_info
                .parse_uri(&raw_uri("#fragment"))
                .as_ref()
                .map(|x| x.url.as_str()),
            Ok("file:///some/pagex.html#fragment")
        );
    }
}
