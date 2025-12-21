use std::borrow::Cow;
use std::sync::LazyLock;

use linkify::LinkFinder;
use reqwest::Url;
use url::ParseError;

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

// Use `LinkFinder` to offload the raw link searching in plaintext
pub(crate) fn find_links(input: &str) -> impl Iterator<Item = linkify::Link<'_>> {
    LINK_FINDER.links(input)
}

pub(crate) trait ReqwestUrlExt {
    fn strip_prefix(&self, prefix: &Url) -> Option<String>;
    fn join_rooted(&self, subpaths: &[&str]) -> Result<Url, ParseError>;
}

impl ReqwestUrlExt for Url {
    fn strip_prefix(&self, prefix: &Url) -> Option<String> {
        let mut prefix_segments = prefix.path_segments()?.peekable();
        let mut url_segments = self.path_segments()?.peekable();

        // strip last component from prefix segments. this will either be
        // a real non-empty filename, or an empty string if prefix ends in `/`.
        let prefix_filename = prefix.path_segments()?.last();

        if prefix_filename.is_some_and(|x| x == "") {
            let _ = prefix_segments.next_back();
        }

        while let Some(s1) = prefix_segments.peek()
            && let Some(s2) = url_segments.peek()
            && s1 == s2
        {
            let _ = prefix_segments.next();
            let _ = url_segments.next();
        }

        let remaining_prefix = prefix_segments.collect::<Vec<&str>>();
        let remaining_url = url_segments.collect::<Vec<&str>>();

        println!("{:?}", remaining_prefix);
        println!("{:?}", remaining_url);

        let relative = match (&remaining_prefix[..], &remaining_url[..]) {
            ([], []) => Some(String::new()),

            // URL is a suffix of prefix (possibly aside from filename).
            // we can just use the rest of the URL.
            ([], rest) => match prefix_filename {
                None | Some("") => rest.join("/"),
                Some(filename) => format!("{filename}/{}", rest.join("/")),
            }.into(),

            _ => None,
        };

        let relative = relative.map(|x| {
            if x.starts_with("/") {
                format!(".{x}")
            } else {
                x
            }
        });

        println!("x={:?}", relative);

        relative
        // prefix
        //     .make_relative(self)
        //     .filter(|subpath| !subpath.starts_with("../") && !subpath.starts_with('/'))
        // .inspect(|x| println!("subpathing {}", x))
        // .filter(|_| prefix.as_str().starts_with(self.as_str()))
    }

    fn join_rooted(&self, subpaths: &[&str]) -> Result<Url, ParseError> {
        let base = self;
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
        // - windows file paths ;-;
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

#[cfg(test)]
mod test_url_ext {
    use super::*;

    macro_rules! url {
        ($x: expr) => {
            Url::parse($x).unwrap()
        };
    }

    #[test]
    fn test_strip_prefix() {
        // note trailing slashes for subpaths, otherwise everything becomes siblings
        let goog = Url::parse("https://goog.com").unwrap();
        let goog_subpath = goog.join("subpath/").unwrap();
        let goog_subsubpath = goog_subpath.join("sub2path/").unwrap();

        assert_eq!(goog.strip_prefix(&goog).as_deref(), Some(""));

        assert_eq!(
            goog_subpath.strip_prefix(&goog).as_deref(),
            Some("subpath/")
        );
        assert_eq!(goog.strip_prefix(&goog_subpath).as_deref(), None);

        assert_eq!(goog_subpath.strip_prefix(&goog_subsubpath).as_deref(), None);
    }

    #[test]
    fn test_fdsa() {
        assert_eq!(
            url!("https://a.com/b/x")
                .strip_prefix(&url!("https://a.com/b/x"))
                .as_deref(),
            Some("")
        );
        assert_eq!(
            url!("https://a.com/b/x")
                .strip_prefix(&url!("https://a.com/b/aa"))
                .as_deref(),
            None
        );
        assert_eq!(
            url!("https://a.com/b/x")
                .strip_prefix(&url!("https://a.com/b/"))
                .as_deref(),
            Some("x")
        );
        assert_eq!(
            url!("https://a.com/b/x")
                .strip_prefix(&url!("https://a.com/b"))
                .as_deref(),
            Some("b/x")
        );
        assert_eq!(
            url!("https://a.com/b/x")
                .strip_prefix(&url!("https://a.com/a"))
                .as_deref(),
            None
        );
        assert_eq!(
            url!("https://a.com/b/x")
                .strip_prefix(&url!("https://a.com/a/"))
                .as_deref(),
            None
        );

        assert_eq!(
            url!("https://a.com/b//x")
                .strip_prefix(&url!("https://a.com/b/"))
                .as_deref(),
            Some("./x")
        );
        assert_eq!(
            url!("https://a.com/b///x")
                .strip_prefix(&url!("https://a.com/b/"))
                .as_deref(),
            Some(".//x")
        );

        println!(
            "{:?}",
            url!("https://a.com/b//x")
                .path_segments()
                .unwrap()
                .collect::<Vec<&str>>()
        );
        println!(
            "{:?}",
            url!("https://a.com/b/")
                .path_segments()
                .unwrap()
                .collect::<Vec<&str>>()
        );
        panic!();
    }
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
