use std::{collections::HashMap, fs::File, io::Read, str::FromStr};

use anyhow::{Context, anyhow};
use clap::Parser;
use regex::Regex;
use reqwest::header::{HeaderMap, USER_AGENT};
use rmcp::{
    ErrorData, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    schemars::JsonSchema,
    tool, tool_handler, tool_router,
    transport::stdio,
};
use roux::{
    MaybeReplies,
    comment::CommentData,
    response::{BasicThing, Listing},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::LazyLock;
use url::Url;

static DOMXREF_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\{\{domxref\("(?P<arg>[^"]+)"\)\}\}"#).unwrap());
static TEMPLATE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{[^}]+\}\}").unwrap());

#[derive(Parser)]
struct Config {
    #[arg(long, env)]
    /// The search engine ID, generated when a new custom search is created in Google
    google_search_engine_id: String,
    #[arg(long, env)]
    /// An API key in Google APIs that has access to the Google Custom Search
    google_search_api_key: String,
    #[arg(long, env)]
    /// The prefix, e.g. the API host and version of the Stack Exchange API
    stack_overflow_api_prefix: String,
    #[arg(long, env)]
    stack_overflow_api_key: Option<String>,
    #[arg(long, env)]
    /// The path where the MDN content github project lives, up to the leading "files" directory
    mdn_base_path: String,
    #[arg(long, env)]
    /// The reddit client id for reddit APIs
    reddit_client_id: String,
    #[arg(long, env)]
    /// The reddit client secret for reddit APIs
    reddit_client_secret: String,
    #[arg(long, env)]
    /// The reddit username (required for Reddit oauth scripts). May create burner account
    reddit_username: String,
    #[arg(long, env)]
    reddit_password: String,
    #[arg(short = 's', long)]
    /// When set, enable Scrapper, the playwright and readability.js based web scraper to fetch
    /// pages without a more specific handler. Set to the host and port of the running Scrapper
    /// server
    /// Warning: Servers may reject traffic or have a CAPTCHA
    scrapper_host: Option<String>,
}

#[derive(Deserialize, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
struct GoogleSearchParams {
    /// a list of words that _all_ must match _exactly_ as a space separated string
    /// generally this should be provided to ensure accurate and on-topic results
    exact_terms: Option<String>,
    /// a list of terms the _must not_ exist in the results as a space separated string
    /// used to filter out unwanted noise that matches the query but isn't relevant
    exclude_terms: Option<String>,
    // max len is 100 results, so no possible need for more than 255
    /// when viewing multiple pages, the offset, or index of the first result
    start: Option<u8>,
    /// the required query itself, the search term(s), as a string. E.g. "typescript enum to string method"
    query: String,
}

#[derive(Deserialize, JsonSchema)]
struct FetchPageParams {
    /// the url of a supported webpage. Must be from a search result or will be invalid
    url: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
    let config = Config::parse();
    let code_tools = Tools::new(config);
    let service = code_tools.serve(stdio()).await.inspect_err(|e| {
        println!("error starting server: {}", e);
    })?;
    service.waiting().await?;
    Ok(())
}

struct Tools {
    config: Config,
    reddit_client: roux::Reddit,
    tool_router: ToolRouter<Self>,
}

#[tool_handler]
impl rmcp::ServerHandler for Tools {
    fn get_info(&self) -> ServerInfo {
        ServerInfo{
			instructions: Some("Search and retrieve web pages in a limited list of sites relevant to software development".to_owned()),
			capabilities: ServerCapabilities::builder().enable_tools().build(),
			..Default::default()
		}
    }
}

#[tool_router]
impl Tools {
    fn new(config: Config) -> Self {
        Self {
            tool_router: Self::tool_router(),
            reddit_client: roux::Reddit::new(
                "linux:nimbus:v0.1.0 (by /u/Keozon)",
                &config.reddit_client_id,
                &config.reddit_client_secret,
            ),
            config,
        }
    }

    #[tool(
        description = "Search a subset of sites in Google for a list of matching web pages with snippets of information"
    )]
    async fn query_google_search(
        &self,
        params: Parameters<GoogleSearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let client = self.get_http_client();
        let base_route = "https://customsearch.googleapis.com/customsearch/v1?";
        let mut builder = client.get(base_route);
        if let Some(exact_terms) = params.0.exact_terms.as_ref() {
            builder = builder.query(&[("exactTerms", &exact_terms)]);
        }
        if let Some(exclude_terms) = params.0.exclude_terms.as_ref() {
            builder = builder.query(&[("excludeTerms", &exclude_terms)]);
        }
        if let Some(start) = params.0.start {
            builder = builder.query(&[("start", &format!("{}", start))]);
        }
        builder = builder
            .query(&[("q", &params.0.query)])
            .query(&[("cx", &self.config.google_search_engine_id)])
            .query(&[("key", &self.config.google_search_api_key)]);
        let res = builder
            .send()
            .await
            .map_err(|err| ErrorData::invalid_params(format!("{}", err), None))?;
        let results: GoogleSearchResults = res
            .json()
            .await
            .map_err(|err| ErrorData::internal_error(format!("{}", err), None))?;
        let json = serde_json::to_string(&results).unwrap();
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    fn get_http_client(&self) -> reqwest::Client {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            "Nimbus Agent/1.0 (reqwest; zbullough@qmulosoft.com)"
                .parse()
                .unwrap(),
        );
        reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap()
    }

    async fn fetch_mdn_page(&self, url: Url) -> Result<String, anyhow::Error> {
        // A URL like https://developer.mozilla.org/en-US/docs/Web/API/Element/mouseover_event
        // maps to a file structure like mdn/files/...
        // just the URL needs lowercased, and the "docs" part needs removed
        let path = url.path().to_ascii_lowercase().replace("/docs/", "/");
        let full_path = format!("{}{}/index.md", self.config.mdn_base_path, path);
        let mut file = File::open(full_path).context("unable to find MDN content at path")?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .context("unable to read mdn page file")?;
        contents = DOMXREF_RE.replace_all(&contents, "`$arg`").to_string();
        Ok(TEMPLATE_RE.replace_all(&contents, "").to_string())
    }

    async fn fetch_reddit_page(
        &self,
        raw_submission_id: &str,
    ) -> Result<Vec<String>, anyhow::Error> {
        let submission_id = format!("t3_{}", raw_submission_id);
        let session = self
            .reddit_client
            .clone()
            .username(&self.config.reddit_username)
            .password(&self.config.reddit_password)
            .login()
            .await?;
        let mut submission = session.get_submissions(&submission_id).await?;
        let submission = submission.data.children.swap_remove(0);
        let title = submission.data.title;
        let contents = submission.data.selftext;
        let likes = submission.data.score;
        let subreddit = submission.data.subreddit;
        let mut thread: Vec<String> = Vec::new();
        let sub = format!(
            "<h1>{}: {}</h1><p>Score/Likes: {}</p><p>{}</p>",
            subreddit, title, likes, contents
        );
        thread.push(sub);
        let comment_client = roux::Subreddit::new_oauth(&subreddit, &session.client);
        let comments = comment_client
            .article_comments(&raw_submission_id, Some(3), Some(20))
            .await
            .context("fetching submission comments")?;
        // use shorter ID names for relationships among comments in this thread
        // this will help smaller models maintain coherence
        let mut contextual_id_map = HashMap::new();
        contextual_id_map.insert(submission_id, 0);
        // TODO make sure the snippet returned from google search is in returned comments
        Self::process_reddit_children(&mut contextual_id_map, &mut thread, comments)?;
        Ok(thread)
    }

    fn process_reddit_children(
        contextual_id_map: &mut HashMap<String, usize>,
        thread: &mut Vec<String>,
        comments: BasicThing<Listing<BasicThing<CommentData>>>,
    ) -> Result<(), anyhow::Error> {
        for comment in comments.data.children.into_iter() {
            let id = comment.data.name.unwrap(); // How could this be null?
            contextual_id_map.insert(id.clone(), contextual_id_map.len());
            if let Some(body) = comment.data.body {
                let id = contextual_id_map.get(&id).unwrap();
                let user = comment.data.author.unwrap_or("unknown redditor".into());
                let link = if let Some(link) = comment.data.permalink {
                    format!("<a href='{}'>Comment Permalink</a>", link)
                } else {
                    format!("")
                };
                let response_to = if let Some(parent) = comment.data.parent_id {
                    let parent = contextual_id_map.get(&parent).unwrap_or(&0);
                    format!(" In response to: {}", parent)
                } else {
                    format!("")
                };
                thread.push(format!(
                    "<h1>Comment: #{} from {}{}</h1>{}<p>{}</p>",
                    id, user, response_to, link, body
                ))
            }
            if let Some(replies) = comment.data.replies {
                match replies {
                    MaybeReplies::Reply(replies) => {
                        Self::process_reddit_children(contextual_id_map, thread, replies)?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    async fn scrape_other_page(&self, url: &Url) -> Result<String, anyhow::Error> {
        let client = self.get_http_client();
        let article_path = format!(
            "{}/api/article",
            self.config.scrapper_host.as_ref().unwrap()
        );
        let res = client
            .get(article_path)
            .query(&[("url", url.to_string()), ("timeout", "10000".to_string())])
            .send()
            .await?;
        let article: ScrapperArticle = res.json().await?;
        Ok(article.text_content)
    }

    async fn fetch_so_page(&self, question_id: &str) -> Result<Vec<String>, anyhow::Error> {
        let client = self.get_http_client();
        let so_questions_path = format!(
            "{}/questions/{}",
            self.config.stack_overflow_api_prefix, question_id
        );
        let so_answers_path = format!(
            "{}/questions/{}/answers",
            self.config.stack_overflow_api_prefix, question_id
        );
        let mut params = vec![
            ("site", "stackoverflow".to_owned()),
            ("filter", "withbody".to_owned()),
        ];
        if let Some(ref key) = self.config.stack_overflow_api_key {
            params.push(("key", key.clone()));
        }
        let res = client
            .get(so_questions_path)
            .query(&params)
            .send()
            .await
            .context("failed to retrieve so question")?;
        let mut question: StackExchangeResponse = res.json().await?;
        if question.items.len() == 0 {
            return Err(anyhow!("SO returned no questions with this ID"));
        }
        let res = client
            .get(so_answers_path)
            .query(&params)
            .send()
            .await
            .context("failed to retrieve so answers")?;
        let answers: StackExchangeResponse = res.json().await?;
        let mut parts = vec![question.items.pop().unwrap()];
        for answer in answers.items.into_iter() {
            parts.push(answer)
        }
        Ok(parts
            .into_iter()
            .map(|part| match part {
                StackExchangeItem::Answer(StackExchangeAnswerFields {
                    common,
                    is_accepted,
                    question_id: _,
                }) => format!(
                    "<h1>{} answer with {} votes</h1><p>{}</p>",
                    if is_accepted {
                        "Accepted"
                    } else {
                        "Unaccepted"
                    },
                    common.score,
                    common.body
                ),
                StackExchangeItem::Question(StackExchangeQuestionFields {
                    common,
                    tags: _,
                    is_answered: _,
                    view_count: _,
                    answer_count: _,
                    link: _,
                    title,
                }) => format!("<h1>{}</h1><p>{}</p>", title, common.body),
            })
            .collect())
    }

    #[tool(
        description = "Retrieve the primary contents of a webpage via its URL, as reterned in a link in a previous search, or from some other source (e.g. user or docs)."
    )]
    async fn fetch_web_page(
        &self,
        params: Parameters<FetchPageParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let parsed = Url::from_str(&params.0.url)
            .map_err(|_| ErrorData::invalid_params("failed to parse url as URL", None))?;
        match parsed.host_str() {
            Some(host) => {
                match host {
                    "stackoverflow.com" => {
                        eprintln!("{}", parsed);
                        let question_id: &str = parsed.path_segments().unwrap().nth(1).ok_or(
                            ErrorData::invalid_params(
                                "invalid stack overflow URL: missing question id",
                                None,
                            ),
                        )?;
                        Ok(CallToolResult::success(
                            self.fetch_so_page(question_id)
                                .await
                                .map_err(|err| ErrorData::internal_error(format!("{}", err), None))?
                                .into_iter()
                                .map(|qa| Content::text(qa))
                                .collect(),
                        ))
                    }
                    "developer.mozilla.org" => Ok(CallToolResult::success(vec![Content::text(
                        self.fetch_mdn_page(parsed)
                            .await
                            .map_err(|err| ErrorData::internal_error(format!("{}", err), None))?,
                    )])),
                    "www.reddit.com" => {
                        let submissision_id = parsed.path_segments().unwrap().nth(3).ok_or(
                            ErrorData::invalid_params(
                                "invalid reddit URL: missing comment/submission id in path",
                                None,
                            ),
                        )?;
                        Ok(CallToolResult::success(
                            self.fetch_reddit_page(submissision_id)
                                .await
                                .map_err(|err| ErrorData::internal_error(format!("{}", err), None))?
                                .into_iter()
                                .map(|comment| Content::text(comment))
                                .collect(),
                        ))
                    }
                    _ if self.config.scrapper_host.is_some() => {
                        Ok(CallToolResult::success(vec![Content::text(
                            self.scrape_other_page(&parsed).await.map_err(|err| {
                                ErrorData::internal_error(format!("{}", err), None)
                            })?,
                        )]))
                    }
                    _ => Err(ErrorData::invalid_params(
                        format!(
                            "invalid host: {}. Must be from provided search results",
                            host
                        ),
                        None,
                    )),
                }
            }
            None => Err(ErrorData::invalid_params(
                "invalid URL: no host component",
                None,
            )),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleSearchResults {
    items: Option<Vec<GoogleSearchResult>>,
    search_information: GoogleSearchInformation,
    queries: GoogleSearchQueryData,
}

#[derive(Deserialize, Serialize)]
struct GoogleSearchResult {
    snippet: String,
    title: String,
    link: String,
    pagemap: PageMap,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleSearchQueryData {
    next_page: Option<Vec<GoogleSearchPage>>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleSearchInformation {
    total_results: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleSearchPage {
    title: String,
    total_results: String,
    count: usize,
    start_index: usize,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(untagged)]
enum PageMap {
    ForumPost(ForumPageMap),
    StackOverflow(StackOverflowPageMap),
    MDN(MDNPageMap),
    Unknown(UnknownPageMap),
}

/// a place holder for when there isn't even a description field in meta
#[derive(Deserialize, Serialize, Debug)]
struct UnknownPageMap {
    metatags: Vec<Value>,
}

#[derive(Deserialize, Serialize, Debug)]
// Encapsulates general, not-customized responses. Works for MDN, Reddit, etc.
struct MDNMeta {
    #[serde(rename = "og:description")]
    description: String,
    #[serde(rename = "og:title")]
    title: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct MDNPageMap {
    metatags: Vec<MDNMeta>,
}

#[derive(Deserialize, Serialize, Debug)]
/// A PageMap that should extract most useful info from forum pages
/// Currently tested on rust user form but hopefully is generic
struct ForumPageMap {
    metatags: Vec<MDNMeta>,
    #[serde(rename = "discussionforumposting")]
    forum_post: Vec<ForumPosting>,
    comment: Option<Vec<ForumPosting>>,
}

#[derive(Deserialize, Serialize, Debug)]
struct ForumPosting {
    #[serde(rename = "articlesection")]
    article_section: Option<String>,
    text: String,
    #[serde(rename = "datepublished")]
    date_published: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct StackOverflowPageMap {
    question: Vec<StackOverflowQuestion>,
    answer: Vec<StackOverflowAnswer>,
}

#[derive(Deserialize, Serialize, Debug)]
/// Represents the StackOverflow Answer in Google Search PageMap
struct StackOverflowAnswer {
    #[serde(rename = "upvotecount")]
    upvote_count: String,
    text: String,
}

#[derive(Deserialize, Serialize, Debug)]
/// Represents the StackOverflow Question in Google Search PageMap
struct StackOverflowQuestion {
    #[serde(rename = "upvotecount")]
    upvote_count: String,
    name: String,
    text: String,
}

#[derive(Serialize, Deserialize)]
struct StackExchangeUser {
    display_name: String,
    reputation: usize,
    link: String,
}

#[derive(Serialize, Deserialize)]
struct StackOverflowCommonFields {
    owner: StackExchangeUser,
    score: usize,
    content_license: String,
    body: String,
}

#[derive(Serialize, Deserialize)]
struct StackExchangeQuestionFields {
    #[serde(flatten)]
    common: StackOverflowCommonFields,
    tags: Vec<String>,
    is_answered: bool,
    view_count: usize,
    link: String,
    answer_count: usize,
    title: String,
}

#[derive(Serialize, Deserialize)]
struct StackExchangeAnswerFields {
    #[serde(flatten)]
    common: StackOverflowCommonFields,
    is_accepted: bool,
    question_id: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum StackExchangeItem {
    Question(StackExchangeQuestionFields),
    Answer(StackExchangeAnswerFields),
}
#[derive(Serialize, Deserialize)]
struct StackExchangeResponse {
    items: Vec<StackExchangeItem>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScrapperArticle {
    text_content: String,
    content: String,
    url: String,
    date: String,
    excerpt: String,
}

#[cfg(test)]
mod test {
    use std::{fs::File, io::Read};

    use super::*;

    #[test]
    fn test_deserialization() {
        let mut data_file = File::open("testdata/sample.json").unwrap();
        let mut data = String::new();
        data_file.read_to_string(&mut data).unwrap();
        let response: GoogleSearchResults = serde_json::from_str(&data)
            .expect("should be able to deserialize from sample response");
        assert_eq!(response.items.as_ref().unwrap().len(), 10);
        assert!(matches!(
            response.items.unwrap()[0].pagemap,
            PageMap::StackOverflow { .. }
        ));

        // Now sample2

        let mut data_file = File::open("testdata/sample2.json").unwrap();
        let mut data = String::new();
        data_file.read_to_string(&mut data).unwrap();
        let response: GoogleSearchResults = serde_json::from_str(&data)
            .expect("should be able to deserialize from sample response");
        assert_eq!(response.items.as_ref().unwrap().len(), 10);
        assert!(matches!(
            response.items.unwrap()[0].pagemap,
            PageMap::MDN { .. }
        ));

        let mut data_file = File::open("testdata/sample3.json").unwrap();
        let mut data = String::new();
        data_file.read_to_string(&mut data).unwrap();
        let response: GoogleSearchResults = serde_json::from_str(&data)
            .expect("should be able to deserialize from sample response");
        assert_eq!(response.items.as_ref().unwrap().len(), 10);
        assert_eq!(
            response.items.as_ref().unwrap()[0].link,
            "https://www.reddit.com/r/rust/comments/ueyt1d/confused_about_how_to_use_tokio_to_process_a/"
        );
        assert!(matches!(
            response.items.unwrap()[0].pagemap,
            PageMap::MDN { .. }
        ));

        let mut data_file = File::open("testdata/sample4.json").unwrap();
        let mut data = String::new();
        data_file.read_to_string(&mut data).unwrap();
        let response: GoogleSearchResults = serde_json::from_str(&data)
            .expect("should be able to deserialize from sample response");
        assert_eq!(response.items.as_ref().unwrap().len(), 10);
        // rust user forum
        assert!(matches!(
            response.items.as_ref().unwrap()[0].pagemap,
            PageMap::ForumPost { .. }
        ));
        // crates.io
        assert!(matches!(
            response.items.unwrap()[1].pagemap,
            PageMap::Unknown { .. }
        ));
    }

    #[test]
    fn test_so_question() {
        let mut data_file = File::open("testdata/so-question.json").unwrap();
        let mut data = String::new();
        data_file.read_to_string(&mut data).unwrap();
        let response: StackExchangeResponse = serde_json::from_str(&data)
            .expect("should be able to deserialize from sample question");
        assert_eq!(response.items.len(), 1);
    }

    #[test]
    fn test_so_answer() {
        let mut data_file = File::open("testdata/so-answer.json").unwrap();
        let mut data = String::new();
        data_file.read_to_string(&mut data).unwrap();
        let response: StackExchangeResponse =
            serde_json::from_str(&data).expect("should be able to deserialize from sample answer");
        assert_eq!(response.items.len(), 1);
    }
}
