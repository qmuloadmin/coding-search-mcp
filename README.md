# Coding Research MCP Server

This server exposes serveral tools to tool-enabled language models to provide more authoritative and up-to-date solutions, recommendations and advice based on software development. It focuses on usability with smaller LLMs (e.g. GLM 4.5 Air, or GPT-OSS 120b) which frequently need their hands held a bit more than the huge proprietary ones. For this reason, documentation provided to the LMs is more extensive than standard tools.

The goal is, combined with a strong system prompt, enable smaller LMs to produce quality content, and not be unable to answer bleeding-edge questions. This is how larger models like Grok 4 and Gemini are able to provide up-to-date information.

## Tools

 - `query_google_search` uses the [Custom Search API](https://programmablesearchengine.google.com/controlpanel/all) in Google to enable LMs to search the web. 
  - Google results often contain enoguh information for the LM to work with, as Google provides snippets for sites like Stack Overflow
 -  `fetch_web_page` is used to retrieve results from google searches, if the snippet is not sufficient. This returns the entire Stack Overflow or MDN article.

## Supported Sources

Currently, the only three sources enabled are the MDN docs (for javascript and web-related APIs), Stack Overflow, and Reddit. 

For unsupported sources, you can connect to an instance of the [Scrapper](https://github.com/amerkurev/scrapper) web scraper, which has some heuristic approach to getting the primary content of a web page (Firefox Reader Mode). This allows for mostly-accurate scraping that doesn't overly contribute to context bloat with navigation, images, advertisements, HTML structure, formatting, etc. However, it is recommended to try it against the sites you want to enable before just assuming it will work, and then setting those verified sites in your Google Custom Search configuration.

Over time additional primary sources will be added and less reliance on Scrapper will be warranted.

## Installing and Running

```
Usage: coding-research-tools [OPTIONS] --google-search-engine-id <GOOGLE_SEARCH_ENGINE_ID> --google-search-api-key <GOOGLE_SEARCH_API_KEY> --stack-overflow-api-prefix <STACK_OVERFLOW_API_PREFIX> --mdn-base-path <MDN_BASE_PATH> --reddit-client-id <REDDIT_CLIENT_ID> --reddit-client-secret <REDDIT_CLIENT_SECRET> --reddit-username <REDDIT_USERNAME> --reddit-password <REDDIT_PASSWORD>

Options:
      --google-search-engine-id <GOOGLE_SEARCH_ENGINE_ID>
          The search engine ID, generated when a new custom search is created in Google [env: GOOGLE_SEARCH_ENGINE_ID=]
      --google-search-api-key <GOOGLE_SEARCH_API_KEY>
          An API key in Google APIs that has access to the Google Custom Search [env: GOOGLE_SEARCH_API_KEY=]
      --stack-overflow-api-prefix <STACK_OVERFLOW_API_PREFIX>
          The prefix, e.g. the API host and version of the Stack Exchange API [env: STACK_OVERFLOW_API_PREFIX=]
      --stack-overflow-api-key <STACK_OVERFLOW_API_KEY>
          [env: STACK_OVERFLOW_API_KEY=]
      --mdn-base-path <MDN_BASE_PATH>
          The path where the MDN content github project lives, up to the leading "files" directory [env: MDN_BASE_PATH=]
      --reddit-client-id <REDDIT_CLIENT_ID>
          The reddit client id for reddit APIs [env: REDDIT_CLIENT_ID=]
      --reddit-client-secret <REDDIT_CLIENT_SECRET>
          The reddit client secret for reddit APIs [env: REDDIT_CLIENT_SECRET=]
      --reddit-username <REDDIT_USERNAME>
          The reddit username (required for Reddit oauth scripts). May create burner account [env: REDDIT_USERNAME=]
      --reddit-password <REDDIT_PASSWORD>
          [env: REDDIT_PASSWORD=]
  -s, --scrapper-host <SCRAPPER_HOST>
          When set, enable Scrapper, the playwright and readability.js based web scraper to fetch pages without a more specific handler. Set to the host and port of the running Scrapper server Warning: Servers may reject traffic or have a CAPTCHA
  -h, --help
          Print help
```

You will need to:

1. Clone the submodule for MDN docs, or provide it in a different location.
2. Create a [Programmable Search Engine](https://developers.google.com/custom-search/docs/tutorial/creatingcse) in Google.
  -  This is free for some (fairly large) number of queries a day.
  -  Limit the sites searched to supported tools. Otherwise results will contain files that can't be retrieved.
  -  You may elect to not filter results, but web scraping won't work in all cases, leading to degraded quality responses from LMs
  -  <img width="649" height="222" alt="image" src="https://github.com/user-attachments/assets/86d8d4d2-63c4-4120-9d73-cd335c8c7809" />

3. Optionally create a Stack Exchange API Key. This will enable more queries per day. It's also free.
4. Obtain a Reddit API Key, and a Reddit account user/pass

If you do not want any of the above features, set them to empty strings for now and ensure that those sites aren't in Google Custom Search. Explicitly setting features as disabled will be supported later.

After the above prerequisites are met, install rust (using rustup is the recommended route) and then:

```bash
cargo build --release
```

Build the project. The binary will be in `targets/release/coding-research-tools`.

You can inspect it with the mcp inspector or you can use it right away. 
