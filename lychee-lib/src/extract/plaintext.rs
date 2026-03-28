use linkify::{LinkFinder, LinkKind};

use crate::types::uri::raw::{RawUri, SpanProvider};

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
        .map(|uri| RawUri {
            // prefix with `mailto:` to avoid invalid emails falling back
            // to relative links.
            text: format!("mailto:{}", uri.as_str()),
            element: None,
            attribute: None,
            span: span_provider.span(uri.start()),
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
}
