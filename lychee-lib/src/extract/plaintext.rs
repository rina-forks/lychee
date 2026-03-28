use linkify::{LinkFinder, LinkKind};

use crate::types::uri::raw::{RawUri, SpanProvider};

/// Cleans an email address returned by Linkify, which is liberal in
/// including non-Unicode characters, into a stricter *extended email autolink* as
/// defined by [Github-flavored Markdown](https://github.github.com/gfm/#extended-email-autolink).
///
/// This involves trimming the hostname of the given raw email, such that it
/// is only alphanumeric or `-` or `_` or `.` and does not end in `-` or `_`.
pub(self) fn clean_linkify_email(raw_email: &str) -> &str {
    let Some((_user, host)) = raw_email.split_once('@') else {
        return raw_email;
    };

    let host_trimmed = host
        .split_once(|c: char| !(c.is_ascii_alphanumeric() || "-_.".contains(c)))
        .map(|x| x.0)
        .unwrap_or(host)
        .trim_end_matches(&['-', '_']);

    let shorten_by = host.len() - host_trimmed.len();
    &raw_email[..raw_email.len() - shorten_by]
}

/// Extract unparsed URL strings from plaintext
pub(crate) fn extract_raw_uri_from_plaintext(
    input: &str,
    span_provider: &impl SpanProvider,
) -> Vec<RawUri> {
    // 1. Find absolute URLs.
    let urls = LinkFinder::new()
        .kinds(&[LinkKind::Url])
        .links(input)
        .map(|uri| RawUri {
            text: uri.as_str().to_owned(),
            element: None,
            attribute: None,
            span: span_provider.span(uri.start()),
        });

    // 2. Find emails. Excluding based on `--include-mail` happens later.
    let emails = LinkFinder::new()
        .kinds(&[LinkKind::Email])
        .links(input)
        .map(|uri| {
            let email = clean_linkify_email(uri.as_str());
            RawUri {
                // prefix with `mailto:` to avoid invalid emails falling back
                // to relative links.
                text: format!("mailto:{}", email),
                element: None,
                attribute: None,
                span: span_provider.span(uri.start()),
            }
        });

    urls.chain(emails).collect()
}

#[cfg(test)]
mod tests {
    use crate::types::uri::raw::{SourceSpanProvider, span};

    use super::*;

    fn extract(input: &str) -> Vec<RawUri> {
        extract_raw_uri_from_plaintext(input, &SourceSpanProvider::from_input(input))
    }

    #[test]
    fn test_extract_local_links() {
        let input = "http://127.0.0.1/ and http://127.0.0.1:8888/ are local links.";
        let links: Vec<RawUri> = extract(input);
        assert_eq!(
            links,
            [
                RawUri::from(("http://127.0.0.1/", span(1, 1))),
                RawUri::from(("http://127.0.0.1:8888/", span(1, 23),)),
            ]
        );
    }

    #[test]
    fn test_extract_link_at_end_of_line() {
        let input = "https://www.apache.org/licenses/LICENSE-2.0\n";
        let uri = RawUri::from((input.trim_end(), span(1, 1)));

        let uris: Vec<RawUri> = extract(input);
        assert_eq!(vec![uri], uris);
    }

    #[test]
    fn test_email_with_unicode() {
        let input = "a@example.com我们  b@example.com- c@example.com，";
        let mut links: Vec<String> = extract(input).into_iter().map(|x| x.text).collect();
        links.sort();
        assert_eq!(
            links,
            [
                "mailto:a@example.com".to_string(),
                "mailto:b@example.com".to_string(),
                "mailto:c@example.com".to_string(),
            ]
        );
    }
}
