use clap::Parser;
use serde::Deserialize;
use serde_json::Value;

#[derive(Parser)]
struct Config {
	#[arg(long, env)]
    google_search_engine_id: String,
	#[arg(long, env)]
    google_search_api_key: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "snake_case")]
struct GoogleSearchParams {
    exact_terms: Option<String>,
    exclude_terms: Option<String>,
    // max len is 100 results, so no possible need for more than 255
    start: Option<u8>,
    query: String,
}

#[tokio::main(flavor="current_thread")]
async fn main() -> Result<(), anyhow::Error>{
	let config = Config::parse();
	let params = GoogleSearchParams{
		query: "on mouse over event handler".into(),
		..Default::default()
	};
	query_google_search(&config, &params).await?;
	Ok(())
}

#[derive(Deserialize)]
struct GoogleSearchResults {
	items: GoogleSearchResult
}

#[derive(Deserialize)]
struct GoogleSearchResult {
	snippet: String,
	title: String,
	
}

#[derive(Deserialize)]
struct StackOverflowPageMap {
	question: Vec<StackOverflowQuestion>,
	answer: Vec<StackOverflowAnswer>
}

#[derive(Deserialize)]
struct StackOverflowAnswer {
	#[serde(rename="upvotecount")]
	upvote_count: String,
	text: String,
}

#[derive(Deserialize)]
struct StackOverflowQuestion { 
	#[serde(rename="upvotecount")]
	upvote_count: String,
	name: String,
	text: String
}

async fn query_google_search(config: &Config, params: &GoogleSearchParams) -> Result<(), anyhow::Error>{
    let client = reqwest::Client::new();
    let base_route = "https://customsearch.googleapis.com/customsearch/v1?";
    let mut builder = client.get(base_route);
    if let Some(exact_terms) = params.exact_terms.as_ref() {
        builder = builder.query(&[("exactTerms", &exact_terms)]);
    }
    if let Some(exclude_terms) = params.exclude_terms.as_ref() {
        builder = builder.query(&[("excludeTerms", &exclude_terms)]);
    }
    if let Some(start) = params.start {
        builder = builder.query(&[("start", &format!("{}", start))]);
    }
    builder = builder
        .query(&[("q", &params.query)])
        .query(&[("cx", &config.google_search_engine_id)])
        .query(&[("key", &config.google_search_api_key)]);
	let res = builder.send().await?;
	let v: Value = res.json().await?;
	println!("{}", v);
	Ok(())
}
