use std::borrow::Cow;
use std::sync::LazyLock;

use linkify::LinkFinder;
use reqwest::Url;

static LINK_FINDER: LazyLock<LinkFinder> = LazyLock::new(LinkFinder::new);

/// Remove all GET parameters from a URL and separates out the fragment.
/// The link is not a URL but a String as it may not have a base domain.
pub(crate) fn remove_get_params_and_separate_fragment(url: &str) -> (&str, Option<&str>) {
    let (path, frag) = match url.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (url, None),
    };
    let path = match path.split_once('?') {
        Some((path_without_params, _params)) => path_without_params,
        None => path,
    };
    (path, frag)
}

pub(crate) fn apply_rooted_base_url(
    base: &Url,
    subpaths: &[&str],
) -> std::result::Result<Url, url::ParseError> {
    // println!("applying {}, {}, {}", base, subpath, link);
    // tests:
    // - .. out of local base should be blocked.
    // - scheme-relative urls should work and not spuriously trigger base url
    // - fully-qualified urls should work
    // - slash should work to go to local base, if specified
    // - slash should be forbidden for inferred base urls.
    // - percent encoding ;-;
    // - trailing slashes in base-url and/or root-dir
    // - fragments and query params, on both http and file
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
    .inspect(|x| println!("---> {x}"))
}

// Use `LinkFinder` to offload the raw link searching in plaintext
pub(crate) fn find_links(input: &str) -> impl Iterator<Item = linkify::Link<'_>> {
    LINK_FINDER.links(input)
}

#[cfg(test)]
mod test_fs_tree {
    use super::*;

    #[test]
    fn test_remove_get_params_and_fragment() {
        assert_eq!(remove_get_params_and_separate_fragment("/"), ("/", None));
        assert_eq!(
            remove_get_params_and_separate_fragment("index.html?foo=bar"),
            ("index.html", None)
        );
        assert_eq!(
            remove_get_params_and_separate_fragment("/index.html?foo=bar"),
            ("/index.html", None)
        );
        assert_eq!(
            remove_get_params_and_separate_fragment("/index.html?foo=bar&baz=zorx?bla=blub"),
            ("/index.html", None)
        );
        assert_eq!(
            remove_get_params_and_separate_fragment("https://example.com/index.html?foo=bar"),
            ("https://example.com/index.html", None)
        );
        assert_eq!(
            remove_get_params_and_separate_fragment("test.png?foo=bar"),
            ("test.png", None)
        );

        assert_eq!(
            remove_get_params_and_separate_fragment("https://example.com/index.html#anchor"),
            ("https://example.com/index.html", Some("anchor"))
        );
        assert_eq!(
            remove_get_params_and_separate_fragment(
                "https://example.com/index.html?foo=bar#anchor"
            ),
            ("https://example.com/index.html", Some("anchor"))
        );
        assert_eq!(
            remove_get_params_and_separate_fragment("test.png?foo=bar#anchor"),
            ("test.png", Some("anchor"))
        );
        assert_eq!(
            remove_get_params_and_separate_fragment("test.png#anchor?anchor!?"),
            ("test.png", Some("anchor?anchor!?"))
        );
        assert_eq!(
            remove_get_params_and_separate_fragment("test.png?foo=bar#anchor?anchor!"),
            ("test.png", Some("anchor?anchor!"))
        );
    }
}
