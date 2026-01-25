use std::borrow::Cow;
use std::sync::LazyLock;

use url::Url;

use linkify::LinkFinder;
use url::ParseError;

pub(crate) trait ReqwestUrlExt {
    /// Joins the given subpaths, using the current URL as the base URL.
    ///
    /// Conceptually, `url.join_rooted(&[path])` is very similar to
    /// `url.join(path)` (using [`Url::join`]). However, they differ when
    /// the base URL is a `file:` URL.
    ///
    /// When used with a `file:` base URL, [`ReqwestUrlExt::join_rooted`]
    /// will ensure that any relative links will *not* traverse outside
    /// of the given base URL. In this way, it is "rooted" at the `file:`
    /// base URL.
    ///
    /// Note that this rooting behaviour only happens for `file:` bases.
    /// Relative links with non-`file:` bases can traverse anywhere as
    /// usual.
    fn join_rooted(&self, subpaths: &[&str]) -> Result<Url, ParseError>;
}

impl ReqwestUrlExt for Url {
    fn join_rooted(&self, subpaths: &[&str]) -> Result<Url, ParseError> {
        let base = self;
        let fake_base = match base.scheme() {
            "file" => {
                let mut fake_base = base.join("/")?;
                fake_base.set_host(Some("secret-lychee-base-url.invalid"))?;
                Some(fake_base)
            }
            _ => None,
        };

        let mut url = Cow::Borrowed(fake_base.as_ref().unwrap_or(base));
        for subpath in subpaths {
            url = Cow::Owned(url.join(subpath)?);
        }

        match fake_base.as_ref().and_then(|b| b.make_relative(&url)) {
            Some(relative_to_base) => base.join(&relative_to_base),
            None => Ok(url.into_owned()),
        }
        // .inspect(|x| println!("---> {x}"))
    }
}

/// Attempts to parse a string which may be a URL or a filesystem path.
/// Returns [`Ok`] if it is a valid URL, or [`Err`] if it is a filesystem path.
///
/// On Windows, we take care to make sure absolute paths---which could also be
/// parsed as URLs---are returned as filesystem paths.
pub(crate) fn parse_url_or_path(input: &str) -> Result<Url, String> {
    match Url::parse(input) {
        Ok(url) if cfg!(windows) && url.scheme().len() == 1 => Err(input.to_string()),
        Ok(url) => Ok(url),
        _ => Err(input.to_string()),
    }
}

static LINK_FINDER: LazyLock<LinkFinder> = LazyLock::new(LinkFinder::new);

// Use `LinkFinder` to offload the raw link searching in plaintext
pub(crate) fn find_links(input: &str) -> impl Iterator<Item = linkify::Link<'_>> {
    LINK_FINDER.links(input)
}

#[cfg(test)]
mod test {
    // TODO: tests for join_rooted
}
