#[cfg(test)]
mod github_fragments {

    fn test_special_casing_html(html: &str) {
        for l in html.lines() {
            if l.contains("class=\"markdown-heading\"") {
                let title = l.split_once("</h1>").unwrap().0.rsplit_once('>').unwrap().1;
                let expected = l
                    .split_once("id=\"user-content-")
                    .unwrap()
                    .1
                    .split_once('"')
                    .unwrap()
                    .0;
                let actual = lychee_lib::extract::fragments::generate_without_disambiguation(title);
                assert_eq!(expected, actual);
            }
        }
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
                test_special_casing_html(&resp.text().await?);
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
