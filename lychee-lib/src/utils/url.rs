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

    /// `url.strictly_relative_to(base) == path` such that
    /// `base.join(path) == url`.
    fn strictly_relative_to(&self, base: &Url, traverse_up: bool) -> Option<String>;
}

impl ReqwestUrlExt for Url {
    fn join_rooted(&self, subpaths: &[&str]) -> Result<Url, ParseError> {
        let base = self;
        let fake_base = match base.scheme() {
            "file" => {
                let mut fake_base = base.join("/.asdjifjdsaiofjdoisa")?;
                fake_base.set_host(Some("secret-lychee-base-url.invalid"))?;
                Some(fake_base)
            }
            _ => None,
        };

        let mut url = Cow::Borrowed(fake_base.as_ref().unwrap_or(base));
        for subpath in subpaths {
            url = Cow::Owned(url.join(subpath)?);
        }

        match fake_base
            .as_ref()
            .and_then(|b| url.strictly_relative_to(b, false))
        {
            Some(relative_to_base) => base.join(&relative_to_base),
            None => Ok(url.into_owned()),
        }
    }

    fn strictly_relative_to(&self, base: &Url, traverse_up: bool) -> Option<String> {
        if self.cannot_be_a_base()
            || base.cannot_be_a_base()
            || self.scheme() != base.scheme()
            || self.authority() != base.authority()
            || self.port() != base.port()
        {
            return None;
        }

        fn filename_with_query(url: &Url) -> String {
            let last = url.path_segments().expect("!cannot_be_a_base").next_back();

            let filename = match last {
                Some("") | None => ".",
                Some(filename) => filename,
            };

            match url.query() {
                Some(query) => format!("{filename}?{query}"),
                None => filename.to_string(),
            }
        }

        let base_filename = filename_with_query(base);
        let self_filename = filename_with_query(self);

        let mut base_segments = base.path_segments().expect("!cannot_be_a_base");
        base_segments.next_back();
        let mut base_segments = base_segments.peekable();

        let mut self_segments = self.path_segments().expect("!cannot_be_a_base");
        self_segments.next_back();
        let mut self_segments = self_segments.peekable();

        while let Some(base_part) = base_segments.peek()
            && let Some(self_part) = self_segments.peek()
            && base_part == self_part
        {
            base_segments.next();
            self_segments.next();
        }

        if base_segments.peek().is_some() && !traverse_up {
            return None;
        }

        let mut remaining = (base_segments.map(|_| ".."))
            .chain(self_segments)
            .collect::<Vec<&str>>();

        let needs_filename = !remaining.is_empty() || self_filename != base_filename;
        let dot_can_be_omitted = !remaining.is_empty();

        if needs_filename {
            remaining.push(if dot_can_be_omitted && self_filename == "." {
                ""
            } else if self_filename.starts_with(".?") {
                self_filename.trim_start_matches('.')
            } else {
                self_filename.as_ref()
            })
        }

        // using "./" is equivalent and makes sure the relative link
        // is not interpreted as a root-relative or scheme-relative link.
        if let Some(first) = remaining.get_mut(0)
            && *first == ""
        {
            *first = "./";
        }

        let mut relative = remaining.join("/");

        if let Some(fragment) = self.fragment() {
            relative.push('#');
            relative.push_str(fragment);
        }

        Some(relative)
    }
}

/// Attempts to parse a string which may be a URL or a filesystem path.
/// Returns [`Ok`] if it is a valid URL, or [`Err`] if it is a filesystem path.
///
/// On Windows, we take care to make sure absolute paths---which could also be
/// parsed as URLs---are returned as filesystem paths.
pub(crate) fn parse_url_or_path(input: &str) -> Result<Url, &str> {
    match Url::parse(input) {
        Ok(url) if cfg!(windows) && url.scheme().len() == 1 => Err(input),
        Ok(url) => Ok(url),
        _ => Err(input),
    }
}

static LINK_FINDER: LazyLock<LinkFinder> = LazyLock::new(LinkFinder::new);

// Use `LinkFinder` to offload the raw link searching in plaintext
pub(crate) fn find_links(input: &str) -> impl Iterator<Item = linkify::Link<'_>> {
    LINK_FINDER.links(input)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_join_rooted() {
        let test_urls_and_expected = [
            // normal HTTP traversal and parsing absolute links
            ("https://a.com/b", vec!["x/", "d"], "https://a.com/x/d"),
            ("https://a.com/b/", vec!["x/", "d"], "https://a.com/b/x/d"),
            (
                "https://a.com/b/",
                vec!["https://new.com", "d"],
                "https://new.com/d",
            ),
            // parsing absolute file://
            ("https://a.com/b/", vec!["file:///a", "d"], "file:///d"),
            ("https://a.com/b/", vec!["file:///a/", "d"], "file:///a/d"),
            (
                "https://a.com/b/",
                vec!["file:///a/b/", "../.."],
                "file:///",
            ),
            // file traversal - should stay within root
            ("file:///a/b/", vec!["a/"], "file:///a/b/a/"),
            ("file:///a/b/", vec!["a/", "../.."], "file:///a/b/"),
            ("file:///a/b/", vec!["a/", "/"], "file:///a/b/"),
            ("file:///a/b/", vec!["/.."], "file:///a/b/"),
            ("file:///a/b/", vec![""], "file:///a/b/"),
            ("file:///a/b/", vec!["."], "file:///a/b/"),
            // HTTP relative links
            ("https://a.com/x", vec![""], "https://a.com/x"),
            ("https://a.com/x", vec![".", "?a"], "https://a.com/?a"),
            ("https://a.com/x", vec!["/"], "https://a.com/"),
            ("https://a.com/x?q#anchor", vec![""], "https://a.com/x?q"),
            ("https://a.com/x#anchor", vec!["?x"], "https://a.com/x?x"),
            // scheme relative link - can traverse outside of root
            ("file:///root/", vec!["///new-root"], "file:///new-root"),
            ("file:///root/", vec!["//a.com/boop"], "file://a.com/boop"),
            ("https://root/", vec!["//a.com/boop"], "https://a.com/boop"),
            // file URLs without trailing / are kinda weird.
            // XXX: the cases with / and . should probably drop the file name
            ("file:///a/b/c", vec!["/../../a"], "file:///a/b/a"),
            ("file:///a/b/c", vec!["/"], "file:///a/b/"),
            ("file:///a/b/c", vec![".?qq"], "file:///a/b/c?qq"), // XXX: still broken...
            ("file:///a/b/c", vec!["#x"], "file:///a/b/c#x"),
            ("file:///a/b/c", vec!["./"], "file:///a/b/"),
            ("file:///a/b/c", vec!["c"], "file:///a/b/c"),
            // joining with d
            ("file:///a/b/c", vec!["d", "/../../a"], "file:///a/b/a"),
            ("file:///a/b/c", vec!["d", "/"], "file:///a/b/"),
            ("file:///a/b/c", vec!["d", "."], "file:///a/b/"),
            ("file:///a/b/c", vec!["d", "./"], "file:///a/b/"),
            // joining with d/
            ("file:///a/b/c", vec!["d/", "/"], "file:///a/b/"),
            ("file:///a/b/c", vec!["d/", "."], "file:///a/b/d/"),
            ("file:///a/b/c", vec!["d/", "./"], "file:///a/b/d/"),
        ];

        for (base, subpaths, expected) in test_urls_and_expected {
            println!("base={base}, subpaths={subpaths:?}, expected={expected}");
            assert_eq!(
                Url::parse(base)
                    .unwrap()
                    .join_rooted(&subpaths[..])
                    .unwrap()
                    .to_string(),
                expected
            );
        }
    }

    #[test]
    fn test_join_default() {
        let test_cases = [
            ("file:///a/b/c", "/", "file:///"),
            ("file:///a/b/c", ".?qq", "file:///a/b/?qq"),
            ("file:///a/b/c", "#x", "file:///a/b/c#x"),
            ("file:///a/b/c", "./", "file:///a/b/"),
        ];

        for (base, subpath, expected) in test_cases {
            println!("base={base}, subpath={subpath:?}, expected={expected}");
            assert_eq!(
                Url::parse(base).unwrap().join(subpath).unwrap().to_string(),
                expected
            );
        }
    }

    #[test]
    fn test_strictly_relative_to_basic() {
        let test_urls = [
            "https://a.com/a/b",
            "https://a.com/a/b2",
            "https://a.com/a",
            "https://a.com",
            "https://a.com/a/",
            "https://a.com/a/b/c/#boop",
            "https://a.com/a/b/c/?query",
            "https://a.com/a/b/c/?QUERY2",
            "https://a.com/a///b/c",
            "https://a.com/x//b/c",
            "https://a.com/x/a",
            "https://a.com/x2/a",
        ];

        for base in test_urls {
            for url in test_urls {
                let base = Url::parse(base).unwrap();
                let url = Url::parse(url).unwrap();

                let result = url.strictly_relative_to(&base, true);

                println!("{url}\tstrictly_relative_to\t{base}\t--> {result:?}");
                println!(
                    "{}",
                    result
                        .as_ref()
                        .and_then(|x| base.join(x).ok())
                        .as_ref()
                        .map_or("", Url::as_str)
                );

                if let Some(result) = result {
                    assert_eq!(base.join(&result).unwrap(), url);
                }
            }
        }
    }

    #[test]
    fn test_strictly_relative_to_doubled() {
        let test_urls = [
            "https://a.com/",
            "https://a.com//",
            "https://a.com///",
            "https://a.com///a",
            "https://a.com/a//",
            "https://a.com//a//b//",
            "https://a.com//a//b//?q",
        ];

        for base in test_urls {
            for url in test_urls {
                let base = Url::parse(base).unwrap();
                let url = Url::parse(url).unwrap();

                let result = url.strictly_relative_to(&base, true);

                println!("{url}\tstrictly_relative_to\t{base}\t--> {result:?}");
                println!(
                    "{}",
                    result
                        .as_ref()
                        .and_then(|x| base.join(x).ok())
                        .as_ref()
                        .map_or("", Url::as_str)
                );

                if let Some(result) = result {
                    assert_eq!(base.join(&result).unwrap(), url);
                }
            }
        }
    }
}
