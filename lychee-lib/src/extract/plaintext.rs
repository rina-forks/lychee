use std::{borrow::Cow, ops::Range, sync::LazyLock};

use regex::{Captures, Regex};

use crate::types::uri::raw::{RawUri, SpanProvider};

static GFM_AUTOLINKS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?x)(?i)

    # https://github.github.com/gfm/#uri-autolink
    < (?P<uri_autolink>
        [a-z][a-z0-9+.-]{1,31} : [^[:space:][:cntrl:]<>]* ) >

    # https://github.github.com/gfm/#email-autolink
    | < (?P<email_autolink>
        [a-zA-Z0-9.!\#$%&'*+/=?^_`{|}~-]+
        @
        [a-zA-Z0-9]
        (?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?
        (?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*
        ) >

    | (?P<extended_www_autolink>
        www\.

        # domain:
        [[:alnum:]_-]+
        (?: \. [[:alnum:]_-]+ )*

        # path:
        [^[:space:]<]* )

    | (?P<extended_url_autolink>
        https?://

        # domain:
        [[:alnum:]_-]+
        (?: \. [[:alnum:]_-]+ )*

        # path:
        [^[:space:]<]* )

    | (?P<extended_email_autolink>
        [[:alnum:]._+-]+
        @
        [[:alnum:]_-]+
        (?: \. [[:alnum:]_-]+ )+ )

    | (?P<extended_protocol_autolink>

        mailto:
        [[:alnum:]._+-]+
        @
        [[:alnum:]_-]+
        (?: \. [[:alnum:]_-]+ )+

        |

        xmpp:
        [[:alnum:]._+-]+
        @
        [[:alnum:]_-]+
        (?: \. [[:alnum:]_-]+ )+
        (?: / [[:alnum:]@.]+ ) )

    "#,
    )
    .expect("gfm autolinks regex invalid")
});

pub enum Autolink<'a> {
    Uri(&'a str),
    Email(&'a str),
    ExtendedWww(&'a str),
    ExtendedUrl(&'a str),
    ExtendedEmail(&'a str),
    ExtendedProtocol(&'a str),
}

impl<'a> Autolink<'a> {
    /// https://github.github.com/gfm/#extended-autolink-path-validation
    fn extended_autolink_path_validation(text: &str) -> &str {
        let mut text = text.trim_end_matches(&['?', '!', '.', ',', ':', '*', '_', '~']);

        if text.ends_with(')') {
            let opens = text.matches('(').count();
            let mut extra_closes = text.match_indices(')').skip(opens);
            if let Some(first_extra_close) = extra_closes.next() {
                text = &text[..first_extra_close.0];
            }
        }

        if text.ends_with(';') {
            if let Some((before, after)) = text.rsplit_once('&') {
                if after.chars().all(|c| c.is_ascii_alphanumeric()) {
                    text = before;
                }
            }
        }

        text
    }

    fn autolink_validation(autolink: Self) -> Option<Self> {
        match autolink {
            Self::Uri(_) | Self::Email(_) => Some(autolink),

            Self::ExtendedWww(link) => Some(Self::ExtendedWww(
                Self::extended_autolink_path_validation(link),
            )),
            Self::ExtendedUrl(link) => Some(Self::ExtendedUrl(
                Self::extended_autolink_path_validation(link),
            )),

            Self::ExtendedEmail(link) | Self::ExtendedProtocol(link)
                if link.ends_with(&['-', '_']) =>
            {
                None
            }

            Self::ExtendedEmail(_) | Self::ExtendedProtocol(_) => Some(autolink),
        }
    }

    /// a
    pub fn raw_text(&self) -> &str {
        match self {
            Autolink::Uri(s)
            | Autolink::Email(s)
            | Autolink::ExtendedWww(s)
            | Autolink::ExtendedUrl(s)
            | Autolink::ExtendedEmail(s)
            | Autolink::ExtendedProtocol(s) => s,
        }
    }

    /// a
    pub fn uri_text(&self) -> Cow<'a, str> {
        match self {
            Autolink::Uri(s) | Autolink::ExtendedUrl(s) | Autolink::ExtendedProtocol(s) => {
                Cow::Borrowed(s)
            }
            Autolink::Email(s) | Self::ExtendedEmail(s) => Cow::Owned(format!("mailto:{s}")),
            Autolink::ExtendedWww(s) => Cow::Owned(format!("http://{s}")),
        }
    }

    fn from_capture(captures: Captures<'a>) -> Option<(Self, Range<usize>)> {
        let autolink = if let Some(cap) = captures.name("uri_autolink") {
            Self::Uri(cap.as_str())
        } else if let Some(cap) = captures.name("email_autolink") {
            Self::Email(cap.as_str())
        } else if let Some(cap) = captures.name("extended_www_autolink") {
            Self::ExtendedWww(cap.as_str())
        } else if let Some(cap) = captures.name("extended_url_autolink") {
            Self::ExtendedUrl(cap.as_str())
        } else if let Some(cap) = captures.name("extended_email_autolink") {
            Self::ExtendedEmail(cap.as_str())
        } else if let Some(cap) = captures.name("extended_protocol_autolink") {
            Self::ExtendedProtocol(cap.as_str())
        } else {
            panic!("regex had incorrect capture groups?!")
        };

        let autolink = Self::autolink_validation(autolink)?;
        let start = captures.get_match().start();
        let end = start + autolink.raw_text().len();
        Some((autolink, Range { start, end }))
    }

    pub fn find(text: &'a str) -> impl Iterator<Item = (Self, Range<usize>)> {
        GFM_AUTOLINKS_REGEX
            .captures_iter(text)
            .filter_map(Self::from_capture)
    }
}

/// Extract unparsed URL strings from plaintext
pub(crate) fn extract_raw_uri_from_plaintext(
    input: &str,
    span_provider: &impl SpanProvider,
) -> Vec<RawUri> {
    Autolink::find(input)
        .map(|(autolink, range)| RawUri {
            text: autolink.uri_text().to_string(),
            element: None,
            attribute: None,
            span: span_provider.span(range.start),
        })
        .collect()
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
    fn test_extract_email() {
        let input = "foo@bar.baz\nhello@mail+xyz.example\nhello+xyz@mail.example";

        let uris: Vec<String> = extract(input).into_iter().map(|x| x.text).collect();
        assert_eq!(
            uris,
            vec![
                "mailto:foo@bar.baz".to_string(),
                "mailto:hello+xyz@mail.example".to_string()
            ]
        );
    }
}
