# Coding Research MCP Server

This server exposes serveral tools to tool-enabled language models to provide more authoritative and up-to-date solutions, recommendations and advice based on software development. It focuses on usability with smaller LLMs (e.g. GLM 4.5 Air, or GPT-OSS 120b) which frequently need their hands held a bit more than the huge proprietary ones. For this reason, documentation provided to the LMs is more extensive than standard tools.

The goal is, combined with a strong system prompt, enable smaller LMs to produce quality content, and not be unable to answer bleeding-edge questions. This is how larger models like Grok 4 and Gemini are able to provide up-to-date information.

## Tools

 - `query_google_search` uses the [Custom Search API](https://programmablesearchengine.google.com/controlpanel/all) in Google to enable LMs to search the web. 
  - Google results often contain enoguh information for the LM to work with, as Google provides snippets for sites like Stack Overflow
 -  `fetch_result_page` is used to retrieve results from google searches, if the snippet is not sufficient. This returns the entire Stack Overflow or MDN article.

## Supported Sources

Currently, the only two sources enabled are the MDN docs (for javascript and web-related APIs) and Stack Overflow. 

I will extend this list over time, and also make it more generic, supporting different technologies in a more agnostic way.

## Installing and Running

```
Usage: coding-research-tools [OPTIONS] --google-search-engine-id <GOOGLE_SEARCH_ENGINE_ID> --google-search-api-key <GOOGLE_SEARCH_API_KEY> --stack-overflow-api-prefix <STACK_OVERFLOW_API_PREFIX> --mdn-base-path <MDN_BASE_PATH>

Options:
      --google-search-engine-id <GOOGLE_SEARCH_ENGINE_ID>
          The search engine ID, generated when a new custom search is created in Google [env: GOOGLE_SEARCH_ENGINE_ID=f535a39ffc8e742ad]
      --google-search-api-key <GOOGLE_SEARCH_API_KEY>
          An API key in Google APIs that has access to the Google Custom Search [env: GOOGLE_SEARCH_API_KEY=AI...dsf]
      --stack-overflow-api-prefix <STACK_OVERFLOW_API_PREFIX>
          The prefix, e.g. the API host and version of the Stack Exchange API [env: STACK_OVERFLOW_API_PREFIX=https://api.stackexchange.com/2.3]
      --stack-overflow-api-key <STACK_OVERFLOW_API_KEY>
          [env: STACK_OVERFLOW_API_KEY=rl_...asdv32]
      --mdn-base-path <MDN_BASE_PATH>
          The path where the MDN content github project lives, up to the leading "files" directory [env: MDN_BASE_PATH=mdn/files]
  -h, --help
          Print help
```

You will need to:

1. Clone the submodule for MDN docs, or provide it in a different location.
2. Create a [Programmable Search Engine](https://developers.google.com/custom-search/docs/tutorial/creatingcse) in Google.
  -  This is free for some (fairly large) number of queries a day.
  -  Limit the sites searched to supported tools. Otherwise results will contain files that can't be retrieved.
  -  <img width="649" height="222" alt="image" src="https://github.com/user-attachments/assets/86d8d4d2-63c4-4120-9d73-cd335c8c7809" />

3. Optionally create a Stack Exchange API Key. This will enable more queries per day. It's also free.

After the above prerequisites are met, install rust (using rustup is the recommended route) and then:

```bash
cargo build --release
```

Build the project. The binary will be in `targets/release/coding-research-tools`.

You can inspect it with the mcp inspector or you can use it right away. 
