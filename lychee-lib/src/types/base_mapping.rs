//! Parses and resolves [`RawUri`] into into fully-qualified [`Uri`] by
//! applying base URL and root dir mappings.
//!

use reqwest::Url;
use std::path::Path;

use crate::Base;
use crate::ErrorKind;
use crate::ResolvedInputSource;
use crate::Uri;
use crate::types::uri::raw::RawUri;
use crate::utils::url::ReqwestUrlExt;
use url::PathSegmentsMut;

/// Information used for resolving relative URLs within a particular
/// input source. There should be a 1:1 correspondence between each
/// `SourceBaseInfo` and its originating `InputSource`. The main entry
/// point for constructing is [`SourceBaseInfo::from_source_url`].
///
/// Once constructed, [`SourceBaseInfo::parse_url_text`] can be used to
/// parse and resolve a (possibly relative) URL obtained from within
/// the associated `InputSource`.
///
/// A `SourceBaseInfo` may be built from input sources which cannot resolve
/// relative links---for instance, stdin. It may also be built from input
/// sources which can resolve *locally*-relative links, but not *root*-relative
/// links.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SourceBaseInfo {
    /// No base information is available. This is for sources with no base
    /// information, such as [`ResolvedInputSource::Stdin`]. This can
    /// resolve no relative links, and only fully-qualified links will be
    /// parsed successfully.
    None,

    /// A base which cannot resolve root-relative links. This is for
    /// `file:` URLs where the root directory is not known. As such, you can
    /// traverse relative to the current URL (by traversing the filesystem),
    /// but you cannot jump to the "root".
    NoRoot(Url),

    /// A full base made up of `origin` and `path`. This can resolve
    /// all kinds of relative links.
    ///
    /// All fully-qualified non-`file:` URLs fall into this case. For these,
    /// `origin` and `path` are obtained by dividing the source URL into its
    /// origin and path. When joined, `${origin}/${path}` should be equivalent
    /// to the source's original URL.
    ///
    /// For `file:` URLs, the `origin` serves as the root which will be used
    /// to resolve root-relative links (i.e., it's the root dir). The `path`
    /// field is the subpath to a particular file within the root dir. This
    /// is retained to resolve locally-relative links.
    Full(Url, String),
}

impl SourceBaseInfo {
    /// Constructs [`SourceBaseInfo::None`].
    pub fn no_info() -> Self {
        Self::None
    }

    /// Constructs [`SourceBaseInfo::Full`] with the given fields.
    pub fn full_info(origin: Url, path: String) -> Self {
        Self::Full(origin, path)
    }

    /// Constructs a [`SourceBaseInfo`], with the variant being determined by the given URL.
    ///
    /// - A [`Url::cannot_be_a_base`] URL will yield [`SourceBaseInfo::None`].
    /// - A `file:` URL will yield [`SourceBaseInfo::NoRoot`].
    /// - For other URLs, a [`SourceBaseInfo::Full`] will be constructed from the URL's
    ///   origin and path.
    ///
    pub fn from_source_url(url: &Url) -> Self {
        if url.scheme() == "file" {
            Self::NoRoot(url.clone())
        } else {
            let mut origin = url.clone();

            match origin.path_segments_mut() {
                Ok(mut segments) => segments.clear(),
                Err(()) => return Self::no_info(),
            };

            let path = match url.path().strip_prefix('/') {
                Some(path) => path.to_string(),
                None => return Self::no_info(),
            };

            Self::Full(origin, path)
        }
    }

    pub fn supports_root_relative(&self) -> bool {
        matches!(self, Self::Full(_, _))
    }

    pub fn supports_locally_relative(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Returns the [`SourceBaseInfo`] which has _more information_
    /// between `self` and the given `fallback`.
    ///
    /// [`SourceBaseInfo::Full`] is preferred over [`SourceBaseInfo::NoRoot`]
    /// which is preferred over [`SourceBaseInfo::None`]. If both `self`
    /// and `fallback` are the same variant, then `self` will be preferred.
    pub fn or_fallback(self, fallback: Self) -> Self {
        match (self, fallback) {
            (x @ Self::Full(_, _), _) => x,
            (_, x @ Self::Full(_, _)) => x,
            (x @ Self::NoRoot(_), _) => x,
            (_, x @ Self::NoRoot(_)) => x,
            (Self::None, Self::None) => Self::None,
        }
    }

    /// Returns whether the text represents a relative link that is
    /// relative to the domain root. Textually, it looks like `/this`.
    fn is_root_relative(text: &str) -> bool {
        let text = text.trim_ascii_start();
        text.starts_with('/') && !text.starts_with("//")
    }

    /// Parses the given URL text into a fully-qualified URL, including
    /// resolving relative links if supported by the current [`SourceBaseInfo`].
    ///
    /// # Errors
    ///
    /// Returns an error if the text is an invalid URL, or the text is a
    /// relative link and this [`SourceBaseInfo`] variant cannot resolve
    /// the relative link.
    pub fn parse_url_text(&self, text: &str) -> Result<Url, ErrorKind> {
        match Uri::try_from(text.as_ref()) {
            Ok(Uri { url }) => Ok(url),
            Err(e @ ErrorKind::ParseUrl(_, _)) => match self {
                Self::NoRoot(_) if Self::is_root_relative(text) => {
                    // TODO: report more errors if a --root-dir is specified but URL falls outside of
                    // thingy
                    Err(ErrorKind::InvalidBaseJoin(text.to_string()))
                }
                Self::NoRoot(base) => base
                    .join_rooted(&[&text])
                    .map_err(|e| ErrorKind::ParseUrl(e, text.to_string())),
                Self::Full(origin, subpath) => origin
                    .join_rooted(&[subpath, &text])
                    .map_err(|e| ErrorKind::ParseUrl(e, text.to_string())),
                Self::None => Err(e),
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
        None => SourceBaseInfo::no_info(),
        Some(fallback_url) => SourceBaseInfo::full_info(fallback_url, String::new()),
    };

    let mappings = UrlMappings::new(root_and_base.into_iter().collect())?;

    let base_info = match source.to_url()? {
        Some(source_url) => match mappings.map_to_old_url(&source_url) {
            Some((remote, subpath)) => SourceBaseInfo::full_info(remote.clone(), subpath),
            None => SourceBaseInfo::from_source_url(&source_url),
        },
        None => SourceBaseInfo::no_info(),
    };

    let base_info = base_info.or_fallback(fallback_base);

    Ok((base_info, mappings))
}

pub fn parse_url_with_base_info(
    base_info: &SourceBaseInfo,
    mappings: &UrlMappings,
    raw_uri: &RawUri,
) -> Result<Uri, ErrorKind> {
    let url = base_info.parse_url_text(&raw_uri.text)?;

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
