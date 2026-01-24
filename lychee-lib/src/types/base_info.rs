//! Parses and resolves [`RawUri`] into into fully-qualified [`Uri`] by
//! applying base URL and root dir mappings.

use reqwest::Url;
use std::borrow::Cow;
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
/// `BaseInfo` and its originating `InputSource`. The main entry
/// point for constructing is [`BaseInfo::from_source_url`].
///
/// Once constructed, [`BaseInfo::parse_url_text`] can be used to
/// parse and resolve a (possibly relative) URL obtained from within
/// the associated `InputSource`.
///
/// A `BaseInfo` may be built from input sources which cannot resolve
/// relative links---for instance, stdin. It may also be built from input
/// sources which can resolve *locally*-relative links, but not *root*-relative
/// links.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum BaseInfo {
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

impl BaseInfo {
    /// Constructs [`BaseInfo::None`].
    pub fn no_info() -> Self {
        Self::None
    }

    /// Constructs [`BaseInfo::Full`] with the given fields.
    pub fn full_info(origin: Url, path: String) -> Self {
        Self::Full(origin, path)
    }

    /// Constructs a [`BaseInfo`], with the variant being determined by the given URL.
    ///
    /// - A [`Url::cannot_be_a_base`] URL will yield [`BaseInfo::None`].
    /// - A `file:` URL will yield [`BaseInfo::NoRoot`].
    /// - For other URLs, a [`BaseInfo::Full`] will be constructed from the URL's
    ///   origin and path.
    pub fn from_source_url(url: &Url) -> Self {
        // TODO: should we return error if a cannot_be_a_base is given?
        if url.scheme() == "file" {
            Self::NoRoot(url.clone())
        } else {
            match Self::split_url_origin_and_path(url) {
                Some((origin, path)) => Self::full_info(origin, path),
                None => Self::no_info(),
            }
        }
    }

    fn split_url_origin_and_path(url: &Url) -> Option<(Url, String)> {
        let origin = url.join("/").ok()?;
        let subpath = origin.make_relative(&url)?;
        Some((origin, subpath))
    }

    pub fn supports_root_relative(&self) -> bool {
        matches!(self, Self::Full(_, _))
    }

    pub fn supports_locally_relative(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Returns the [`BaseInfo`] which has _more information_
    /// between `self` and the given `fallback`.
    ///
    /// [`BaseInfo::Full`] is preferred over [`BaseInfo::NoRoot`]
    /// which is preferred over [`BaseInfo::None`]. If both `self`
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
    /// resolving relative links if supported by the current [`BaseInfo`].
    ///
    /// # Errors
    ///
    /// Returns an error if the text is an invalid URL, or if the text is a
    /// relative link and this [`BaseInfo`] variant cannot resolve
    /// the relative link.
    pub fn parse_url_text(&self, text: &str, root_dir: Option<&Url>) -> Result<Url, ErrorKind> {
        // HACK: if root-dir is specified, apply it by fudging around with
        // file:// URLs.
        let fake_base_info = match root_dir {
            Some(root_dir) if Self::is_root_relative(text) => {
                Cow::Owned(Self::full_info(root_dir.clone(), String::new()))
            }
            Some(_) | None => Cow::Borrowed(self),
        };

        match Uri::try_from(text.as_ref()) {
            Ok(Uri { url }) => Ok(url),
            Err(e @ ErrorKind::ParseUrl(_, _)) => match *fake_base_info {
                Self::NoRoot(_) if Self::is_root_relative(text) => {
                    Err(ErrorKind::InvalidBaseJoin(text.to_string()))
                }
                Self::NoRoot(ref base) => base
                    .join_rooted(&[text])
                    .map_err(|e| ErrorKind::ParseUrl(e, text.to_string())),
                Self::Full(ref origin, ref subpath) => origin
                    .join_rooted(&[subpath, text])
                    .map_err(|e| ErrorKind::ParseUrl(e, text.to_string())),
                Self::None => Err(e),
            },
            Err(e) => Err(e),
        }
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

    // #[test]
    // fn test_base_with_filename() {
    //     let root_dir = PathBuf::from("/some");
    //     let base = Base::try_from("https://example.com/path/page2.html").unwrap();
    //     let source = ResolvedInputSource::FsPath(PathBuf::from("/some/page.html"));
    //     let base_info =
    //         BaseInfo::from_source(&source, Some((&root_dir, Some(&base))), None).unwrap();
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
    //         BaseInfo::from_source(&source, Some((&root_dir, Some(&base))), None).unwrap();
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
