use crate::{Error, Result};
use itertools::Itertools;
use scraper::Html as ScraperHtml;
use scraper::{Node, Selector};
use std::collections::BTreeMap;

use crate::schema::{Field, ALL_FIELDS};

pub struct Webpage {
    pub html: Html,
    pub backlinks: Vec<Link>,
    pub centrality: f64,
}

impl Webpage {
    pub fn new(html: &str, url: &str, backlinks: Vec<Link>, centrality: f64) -> Self {
        let html = Html::parse(html, url);

        Self {
            html,
            backlinks,
            centrality,
        }
    }

    pub fn into_tantivy(self, schema: &tantivy::schema::Schema) -> Result<tantivy::Document> {
        let mut doc = self.html.into_tantivy(schema)?;

        let backlink_text: String = itertools::intersperse(
            self.backlinks.into_iter().map(|link| link.text),
            "\n".to_string(),
        )
        .collect();

        doc.add_text(
            schema
                .get_field(Field::BacklinkText.as_str())
                .expect("Failed to get backlink-text field"),
            backlink_text,
        );

        doc.add_f64(
            schema
                .get_field(Field::Centrality.as_str())
                .expect("Failed to get centrality field"),
            self.centrality,
        );

        Ok(doc)
    }
}

#[derive(Debug)]
pub struct Html {
    dom: ScraperHtml,
    url: String,
}

impl Html {
    pub fn parse(html: &str, url: &str) -> Self {
        Self {
            dom: ScraperHtml::parse_document(html),
            url: url.to_string(),
        }
    }

    pub fn links(&self) -> Vec<Link> {
        let selector = Selector::parse("a").expect("Failed to parse selector");
        self.dom
            .select(&selector)
            .map(|el| {
                let destination = el.value().attr("href");
                let text = el.text().collect::<String>();

                (destination, text)
            })
            .filter(|(dest, _)| dest.is_some())
            .map(|(destination, text)| {
                let destination = destination.unwrap().to_string();
                Link {
                    source: self.url.clone(),
                    destination,
                    text,
                }
            })
            .collect()
    }

    fn grab_texts(&self, selector: &Selector) -> Vec<String> {
        self.dom
            .select(selector)
            .filter(|el| selector.matches(el))
            .filter_map(|el| {
                if let Some(node) = (*el).first_child() {
                    if let Node::Text(text) = node.value() {
                        Some(text)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .map(|t| String::from(t.trim()))
            .filter(|t| !t.is_empty())
            .collect::<Vec<String>>()
    }

    pub fn text(&self) -> String {
        let selector = Selector::parse(
            "body a,
            body div,
            body span,
            body p,
            body h1,
            body h2,
            body h3,
            body h4,
            body li,
            body ul,
            body ol,
            body nav,
            body pre,
            body
            ",
        )
        .expect("Failed to parse selector");
        Itertools::intersperse(self.grab_texts(&selector).into_iter(), "\n".to_string())
            .collect::<String>()
            .trim()
            .to_string()
    }

    pub fn title(&self) -> Option<String> {
        let selector = Selector::parse("title").expect("Failed to parse selector");
        self.grab_texts(&selector).get(0).cloned()
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn host(&self) -> &str {
        let mut start_host = 0;
        if self.url().starts_with("http://") || self.url().starts_with("https://") {
            start_host = self
                .url()
                .find('/')
                .expect("It was checked that url starts with protocol");
            start_host += 2; // skip the two '/'
        }

        let mut end_host = self.url.len();
        if self.url()[start_host..].contains('/') {
            end_host = self.url()[start_host..]
                .find('/')
                .expect("The url contains atleast 1 '/'");
            end_host += start_host; // offset the start position
        }

        &self.url()[start_host..end_host]
    }

    pub fn metadata(&self) -> Vec<Meta> {
        let selector = Selector::parse("meta").expect("Failed to parse selector");
        self.dom
            .select(&selector)
            .map(|el| {
                el.value()
                    .attrs()
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect::<BTreeMap<String, String>>()
            })
            .collect()
    }

    pub fn into_tantivy(self, schema: &tantivy::schema::Schema) -> Result<tantivy::Document> {
        let mut doc = tantivy::Document::new();

        for field in &ALL_FIELDS {
            let tantivy_field = schema
                .get_field(field.as_str())
                .unwrap_or_else(|| panic!("Unknown field: {}", field.as_str()));

            match field {
                Field::Title => {
                    if self.title().is_none() {
                        return Err(Error::EmptyField("title"));
                    }

                    doc.add_text(tantivy_field, self.title().unwrap())
                }
                Field::Body => doc.add_text(tantivy_field, self.text()),
                Field::Url => doc.add_text(tantivy_field, self.url()),
                Field::BacklinkText | Field::Centrality => {}
            }
        }

        Ok(doc)
    }
}

#[derive(Debug, PartialEq)]
pub struct Link {
    pub source: String,
    pub destination: String,
    pub text: String,
}

pub type Meta = BTreeMap<String, String>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() {
        let raw = r#"
            <html>
                <head>
                    <title>Best website</title>
                    <meta name="meta1" content="value">
                </head>
                <body>
                    <a href="example.com">Best example website ever</a>
                </body>
            </html>
        "#;

        let webpage = Html::parse(raw, "https://www.example.com/whatever");

        assert_eq!(&webpage.text(), "Best example website ever");
        assert_eq!(webpage.title(), Some("Best website".to_string()));
        assert_eq!(
            webpage.links(),
            vec![Link {
                source: "https://www.example.com/whatever".to_string(),
                destination: "example.com".to_string(),
                text: "Best example website ever".to_string()
            }]
        );

        let mut expected_meta = BTreeMap::new();
        expected_meta.insert("name".to_string(), "meta1".to_string());
        expected_meta.insert("content".to_string(), "value".to_string());

        assert_eq!(webpage.metadata(), vec![expected_meta]);
        assert_eq!(webpage.url(), "https://www.example.com/whatever");
        assert_eq!(webpage.host(), "www.example.com");
    }

    #[test]
    fn text_raw_body() {
        let raw = r#"
            <html>
                <body>
                    test website
                </body>
            </html>
        "#;

        let webpage = Html::parse(raw, "https://www.example.com/whatever");

        assert_eq!(&webpage.text(), "test website");
    }

    #[test]
    fn script_tags_text_ignored() {
        let raw = r#"
            <html>
                <head>
                    <title>Best website</title>
                    <meta name="meta1" content="value">
                    <script>this should not be extracted</script>
                </head>
                <body>
                    <script>this should not be extracted</script>
                    <p>This text should be the first text extracted</p>
                    <div>
                        <script>this should not be extracted</script>
                        <p>This text should be the second text extracted</p>
                    </div>
                    <script>this should not be extracted</script>
                </body>
            </html>
        "#;

        let webpage = Html::parse(raw, "https://www.example.com");

        assert_eq!(
            webpage.text(),
            "This text should be the first text extracted\nThis text should be the second text extracted"
        );
    }

    #[test]
    fn style_tags_text_ignored() {
        let raw = r#"
            <html>
                <head>
                    <title>Best website</title>
                    <meta name="meta1" content="value">
                    <style>this should not be extracted</style>
                </head>
                <body>
                    <style>this should not be extracted</style>
                    <p>This text should be the first text extracted</p>
                    <div>
                        <style>this should not be extracted</style>
                        <p>This text should be the second text extracted</p>
                    </div>
                    <style>this should not be extracted</style>
                </body>
            </html>
        "#;

        let webpage = Html::parse(raw, "https://www.example.com");

        assert_eq!(
            webpage.text(),
            "This text should be the first text extracted\nThis text should be the second text extracted"
        );
    }
}
