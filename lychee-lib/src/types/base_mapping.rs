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
pub struct SourceBaseInfo(Option<(Url, String, bool)>);
/// Tuple of `origin`, `subpath`, `allow_absolute`. The field `allow_absolute`
/// is true if the base is well-founded.

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

impl SourceBaseInfo {
    pub fn new(
        origin: Url,
        subpath: String,
        supports_root_relative: bool,
    ) -> Result<SourceBaseInfo, ErrorKind> {
        Ok(Self(Some((origin, subpath, supports_root_relative))))
    }

    pub fn none() -> Self {
        Self(None)
    }

    pub fn supports_root_relative(&self) -> bool {
        self.0.as_ref().is_some_and(|x| x.2)
    }

    pub fn or_fallback(self, fallback: Self) -> Self {
        if self.supports_root_relative() {
            self
        } else {
            fallback
        }
    }

    pub fn infer_source_base(url: &Url) -> Result<Self, ErrorKind> {
        let origin = url
            .join("/")
            .map_err(|e| ErrorKind::ParseUrl(e, url.to_string()))?;
        let subpath = origin
            .make_relative(url)
            .expect("failed make a url relative to its own origin root?!");
        Self::new(origin, subpath, url.scheme() != "file")
    }

    pub fn parse_raw_uri(&self, raw_uri: &RawUri) -> Result<Url, ErrorKind> {
        match Uri::try_from(raw_uri.text.as_ref()) {
            Ok(Uri { url }) => Ok(url),
            Err(e @ ErrorKind::ParseUrl(_, _)) => match self {
                _ if raw_uri.is_root_relative() && !self.supports_root_relative() => {
                    // TODO: report more errors if a --root-dir is specified but URL falls outside of
                    // thingy
                    Err(ErrorKind::InvalidBaseJoin(raw_uri.text.clone()))
                }
                Self(Some((origin, subpath, _))) => origin
                    .join_rooted(&[subpath, &raw_uri.text])
                    .map_err(|e| ErrorKind::ParseUrl(e, raw_uri.text.clone())),
                Self(None) => Err(e),
            },
            Err(e) => Err(e),
        }
    }

    // Constructs a `SourceBaseInfo` from the given input source, root and base
    // pair, and fallback base.
    //
    // # Arguments
    //
    // * `source` - The input source which contains the links we want to resolve.
    // * `root_and_base` - An optional pair of root directory and base URL. The
    //   somewhat complicated type encodes the fact that if a [`Base`] is provided,
    //   then a [`Path`] must be provided too. If the base URL is omitted but root
    //   dir is provided, the base URL defaults to the root dir.
    // * `fallback_base` - A fallback base URL to use where no other well-founded
    //   base URL can be derived. If it is applied, the fallback base URL is
    //   considered to be a well-founded base.
    //
    // # Root and base
    //
    // The given root and base URL are used to transform the intrinsic base returned
    // by [`InputSource::to_url`]. If the intrinsic base is a subpath of the given
    // root, then a new base is constructed by taking the intrinsic base and replacing
    // the root dir with the given base URL.
    //
    // In this way, links from local files can be resolved *as if* they were hosted
    // in a remote location at the base URL. Later, in [`SourceBaseInfo::parse_uri`],
    // remote links which are subpaths of the base URL will be reflected back to
    // local files within the root dir.
    //
    // # Well-founded bases
    //
    // Formally, a *well-founded* base is one which is derived from an input
    // source which is *not* a local file, or one derived from a local file
    // source which is a descendent of the given root dir.
    //
    // Informally, and importantly for using [`SourceBaseInfo`], a well-founded
    // base is one where we can sensibly resolve root-relative links (i.e.,
    // relative links starting with `/`).
    //
    // # Errors
    //
    // This function fails with an [`Err`] if:
    // - any of the provided arguments cannot be converted to a URL, or
    // - [`SourceBaseInfo::new`] fails.
}

pub fn prepare_source_base_info(
    source: &ResolvedInputSource,
    root_and_base: Option<(&Path, Option<&Base>)>,
    fallback_base: Option<&Base>,
) -> Result<(SourceBaseInfo, UrlMappings), ErrorKind> {
    let root_and_base: Option<(Url, Url)> = match root_and_base {
        // if root is specified but not base, use root dir as the base as well.
        Some((root, base_option)) => {
            let root = Base::Local(root.to_owned()).to_url()?;
            let base = base_option.map_or_else(|| Ok(root.clone()), Base::to_url)?;
            Some((root, base))
        }
        None => None,
    };

    let fallback_base = match fallback_base.map(Base::to_url).transpose()? {
        None => SourceBaseInfo::none(),
        Some(fallback_url) => SourceBaseInfo::new(fallback_url, String::new(), true)?,
    };

    let mappings = UrlMappings::new(root_and_base.into_iter().collect())?;

    let base_info = match source.to_url()? {
        Some(source_url) => match mappings.map_to_old_url(&source_url) {
            Some((remote, subpath)) => SourceBaseInfo::new(remote.clone(), subpath, true)?,
            None => SourceBaseInfo::infer_source_base(&source_url)?,
        },
        None => SourceBaseInfo::none(),
    };

    let base_info = base_info.or_fallback(fallback_base);

    Ok((base_info, mappings))
}

pub fn parse_url_with_base_info(
    base_info: &SourceBaseInfo,
    mappings: &UrlMappings,
    raw_uri: &RawUri,
) -> Result<Uri, ErrorKind> {
    let url = base_info.parse_raw_uri(raw_uri)?;

    let mut url = match mappings.map_to_new_url(&url) {
        Some((local, subpath)) => local.join(&subpath).ok(),
        None => None,
    }
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

    // #[test]
    // fn test_base_with_filename() {
    //     let root_dir = PathBuf::from("/some");
    //     let base = Base::try_from("https://example.com/path/page2.html").unwrap();
    //     let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));
    //     let base_info =
    //         SourceBaseInfo::from_source(&source, Some((&root_dir, Some(&base))), None).unwrap();
    //
    //     assert_eq!(
    //         base_info
    //             .parse_uri(&raw_uri("#fragment"))
    //             .as_ref()
    //             .map(|x| x.url.as_str()),
    //         Ok("file:///some/page.html#fragment")
    //     );
    // }
    //
    // #[test]
    // fn test_base_with_same_filename() {
    //     let root_dir = PathBuf::from("/some/pagex.html");
    //     let base = Base::try_from("https://example.com/path/page.html").unwrap();
    //     let source = ResolvedInputSource::FsPath(PathBuf::from("/some/pagex.html"));
    //     let base_info =
    //         SourceBaseInfo::from_source(&source, Some((&root_dir, Some(&base))), None).unwrap();
    //
    //     assert_eq!(
    //         base_info
    //             .parse_uri(&raw_uri("#fragment"))
    //             .as_ref()
    //             .map(|x| x.url.as_str()),
    //         Ok("file:///some/pagex.html#fragment")
    //     );
    // }
}
