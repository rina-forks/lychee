#[cfg(test)]
mod github_fragments {
    use lychee_lib::extract::fragments::generate_without_disambiguation;

    /// Given HTML rendered by the Github API, this will extract
    /// `(title, expected_fragment)` pairs.
    ///
    /// The HTML input has this format for Markdown fragment headings
    /// (line breaks added for clarity and are not in the original HTML).
    ///
    /// ```html
    /// <div class="markdown-heading" dir="auto">
    ///   <h1 class="heading-element" dir="auto">ß-ß-Ss-SS- # LATIN SMALL LETTER SHARP S</h1>
    ///   <a id="..." class="anchor" aria-label="..." href="#ß-ß-ss-ss---latin-small-letter-sharp-s">
    ///   ...
    /// ```
    fn extract_special_casing_html(html: &str) -> impl std::iter::Iterator<Item = (&str, &str)> {
        fn extract_line(l: &str) -> (&str, &str) {
            let title = l.split_once("</h1>").unwrap().0.rsplit_once('>').unwrap().1;
            let href = "href=\"#";
            let id = l.split_once(href).unwrap().1.split_once('"').unwrap().0;
            (title, id)
        }

        html.lines()
            .filter(|l| l.contains("class=\"markdown-heading\""))
            .map(extract_line)
    }

    /// Tests by using the Github API to render the HTML of a Markdown file
    /// which has been committed to the repo.
    ///
    /// See `fixtures/fragments/make-special-casing.md` for how to generate that
    /// markdown file.
    #[tokio::test]
    async fn test_github_fragments_live() -> Result<(), reqwest::Error> {
        let api_url = "https://api.github.com/repos/lycheeverse/lychee/contents";
        let file_path = "/fixtures/fragments/special-casing.md";
        let commit = "0788e393989c4f1f747529324189a8a74d6f2e96";

        let mut auth_header = reqwest::header::HeaderMap::new();
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            let token = format!("Bearer {token}").try_into().unwrap();
            auth_header.insert("Authorization", token).unwrap();
        }

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("{api_url}{file_path}?ref={commit}"))
            .header("Accept", "application/vnd.github.html+json")
            .header("X-GitHub-Api-Version", "2026-03-10")
            .header("User-Agent", "Lychee-Unit-Test")
            .headers(auth_header)
            .send()
            .await;

        match resp.and_then(reqwest::Response::error_for_status) {
            Ok(resp) => {
                extract_special_casing_html(&resp.text().await?).for_each(|(title, expected)| {
                    assert_eq!(expected, generate_without_disambiguation(title));
                });
                Ok(())
            }
            Err(err)
                if err
                    .status()
                    .is_some_and(|c| c == http::StatusCode::TOO_MANY_REQUESTS) =>
            {
                println!("Ignoring 429 in live Github fragment slugify test");
                Ok(())
            }
            Err(err) => Err(err),
        }
    }
}
