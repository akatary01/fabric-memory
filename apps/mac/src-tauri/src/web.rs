//! Read-only public-web tools, using the Mac's local SearXNG service.
use reqwest::{blocking::Client, Url};
use scraper::{Html, Node, Selector};
use serde_json::{json, Value};
use std::{
    io::Read,
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    time::Duration,
};
const LIMIT: u64 = 2 * 1024 * 1024;

pub fn urls(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|s| {
            s.trim_matches(|c: char| {
                matches!(c, '(' | ')' | '<' | '>' | '"' | '\'' | ',' | '.' | ';')
            })
        })
        .filter(|s| s.starts_with("https://") || s.starts_with("http://"))
        .take(2)
        .map(String::from)
        .collect()
}
fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_documentation()
                || a == 0
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 198 && (18..=19).contains(&b))
                || (a == 192 && b == 0))
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return public(IpAddr::V4(v4));
            }
            let s = ip.segments();
            s[0] & 0xe000 == 0x2000
                && s[0] != 0x2002
                && !(s[0] == 0x2001 && (s[1] == 0 || s[1] == 0x0db8))
        }
    }
}
fn resolve(url: &Url) -> Result<Vec<SocketAddr>, String> {
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(
            "Web requests require a public HTTP(S) URL without embedded credentials".into(),
        );
    }
    let port = url.port_or_known_default().ok_or("Missing web port")?;
    if !matches!(port, 80 | 443) {
        return Err("Web requests allow only public HTTP/HTTPS ports".into());
    }
    let host = url
        .host_str()
        .ok_or("Missing web host")?
        .trim_matches(['[', ']']);
    let addresses: Vec<_> = (host, port)
        .to_socket_addrs()
        .map_err(|_| "Could not resolve web host")?
        .take(16)
        .collect();
    if addresses.is_empty() || addresses.iter().any(|v| !public(v.ip())) {
        return Err("Web access to private, loopback, or reserved addresses is blocked".into());
    }
    Ok(addresses)
}
fn http(url: &Url) -> Result<Client, String> {
    let addresses = resolve(url)?;
    Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(url.host_str().unwrap().trim_matches(['[', ']']), &addresses)
        .user_agent("MemoryFabric/0.1 (read-only web retrieval)")
        .connect_timeout(Duration::from_secs(5))
        .timeout(crate::agent::bounded_timeout(15)?)
        .build()
        .map_err(|_| "Cannot initialize web reader".into())
}
fn body(response: reqwest::blocking::Response) -> Result<String, String> {
    if response.content_length().is_some_and(|size| size > LIMIT) {
        return Err("Web response exceeds 2 MB limit".into());
    }
    let mut bytes = Vec::new();
    response
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read web response")?;
    if bytes.len() as u64 > LIMIT {
        return Err("Web response exceeds 2 MB limit".into());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
fn plain(text: &str, limit: usize) -> String {
    Html::parse_fragment(text)
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}
fn page(html: &str) -> (String, String) {
    let document = Html::parse_document(html);
    let title = document
        .select(&Selector::parse("title").unwrap())
        .next()
        .map(|e| e.text().collect::<String>())
        .unwrap_or_default();
    let selector = Selector::parse("main, article").unwrap();
    let root = document
        .select(&selector)
        .next()
        .unwrap_or_else(|| document.root_element());
    let mut text = String::new();
    for node in root.descendants().take(80_000) {
        if let Node::Text(value) = node.value() {
            let hidden = node.ancestors().any(|a| {
                a.value().as_element().is_some_and(|e| {
                    matches!(
                        e.name(),
                        "script" | "style" | "noscript" | "nav" | "footer" | "header"
                    )
                })
            });
            if !hidden {
                text.extend(value.text.chars().take(6000));
                text.push(' ');
            }
            if text.len() > 24_000 {
                break;
            }
        }
    }
    (
        title.chars().take(240).collect(),
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(6000)
            .collect(),
    )
}
fn aggregate_rating(html: &str) -> Option<Value> {
    fn find(value: &Value) -> Option<Value> {
        if let Some(items) = value.as_array() { return items.iter().find_map(find); }
        let aggregate = &value["aggregateRating"];
        let number = |field: &Value| field.as_f64().or_else(|| field.as_str()?.parse().ok());
        if let Some(rating) = number(&aggregate["ratingValue"]) {
            let best = number(&aggregate["bestRating"]).unwrap_or(5.0);
            if best.is_finite() && best > 0.0 && rating >= 0.0 && rating <= best {
                return Some(json!({"out_of_five":rating * 5.0 / best,"value":rating,"scale":best}));
            }
        }
        find_graph(value)
    }
    fn find_graph(value: &Value) -> Option<Value> {
        value.get("@graph").and_then(find)
    }
    let document = Html::parse_document(html);
    let selector = Selector::parse("script[type='application/ld+json']").ok()?;
    document.select(&selector).find_map(|node| {
        let value: Value = serde_json::from_str(&node.inner_html()).ok()?;
        find(&value)
    })
}

pub fn fetch(address: &str) -> Result<Value, String> {
    let mut url = Url::parse(address).map_err(|_| "Invalid web URL")?;
    for _ in 0..4 {
        let response = http(&url)?
            .get(url.clone())
            .send()
            .map_err(|_| "Web page request failed")?;
        if response.status().is_redirection() {
            let next = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or("Invalid web redirect")?;
            url = url.join(next).map_err(|_| "Invalid redirect URL")?;
            continue;
        }
        if !response.status().is_success() {
            return Err(format!(
                "Web page returned HTTP {}",
                response.status().as_u16()
            ));
        }
        let kind = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();
        if !(kind.contains("text/") || kind.contains("application/json")) {
            return Err("Web reader supports HTML, text and JSON responses".into());
        }
        let raw = body(response)?;
        let (title, text) = if kind.contains("html") {
            page(&raw)
        } else {
            (
                url.host_str().unwrap_or("Web page").into(),
                raw.chars().take(6000).collect(),
            )
        };
        if text.trim().is_empty() {
            return Err("Web page contains no readable text".into());
        }
        let rating = if kind.contains("html") { aggregate_rating(&raw) } else { None };
        let content = match &rating {
            Some(rating) => format!("{text}\nPublished aggregate rating: {} out of {}.", rating["value"], rating["scale"]),
            None => text,
        };
        return Ok(json!({"title":title,"url":url.as_str(),"content":content,"kind":"page","rating":rating}));
    }
    Err("Web page exceeded redirect limit".into())
}
pub fn open_source(address: &str) -> Result<(), String> {
    let url = Url::parse(address).map_err(|_| "Invalid source URL")?;
    resolve(&url)?;
    let status = std::process::Command::new("open")
        .arg(url.as_str())
        .status()
        .map_err(|_| "Could not open source")?;
    if status.success() {
        Ok(())
    } else {
        Err("Could not open source in browser".into())
    }
}
fn search_query(question: &str) -> String {
    let text=question.trim(); let lower=text.to_lowercase();
    for phrase in ["most recent news", "latest news", "recent news"] {
        if let Some(i)=lower.find(phrase) {
            let tail=text[i+phrase.len()..].trim_matches(|c:char| c.is_whitespace() || matches!(c,'?'|'.'|'!'));
            return format!("latest news {tail}").trim().to_string();
        }
    }
    for prefix in ["search google for ","search the web for ","look up ","search for ","google "] {
        if lower.starts_with(prefix) { return text[prefix.len()..].trim().into(); }
    }
    text.into()
}
fn relevant(query:&str,title:&str,snippet:&str)->bool {
    let lower=query.to_lowercase();
    let stop=["the","and","for","with","from","this","that","what","which","how","find","search","latest","news","please","today","give","show","look","web","me"];
    let mut terms=lower.split(|c:char|!c.is_alphanumeric()).filter(|t|t.len()>2 && !stop.contains(t)).collect::<Vec<_>>();terms.sort();terms.dedup();
    if terms.len()<2{return true;}
    let evidence=format!("{title} {snippet}").to_lowercase();
    terms.iter().filter(|term|evidence.contains(**term)).count()>=2
}
pub fn research(question: &str) -> Result<Vec<Value>, String> {
    if !urls(question).is_empty() {
        return gather(question);
    }
    let model = std::env::var("FABRIC_WEB_MODEL").unwrap_or_else(|_| "lfm3-vl:latest".into());
    let messages = json!([
        {"role":"system","content":"Turn the request into one public search query. Keep the original topic, names and dates. Do not answer or add new facts. Return a JSON object with a query string."},
        {"role":"user","content":question}
    ]);
    let schema = json!({"type":"object","required":["query"],"properties":{"query":{"type":"string"}}});
    let query = crate::generation::request_model(&model, messages, Some(schema), 96, 12, "60s", 2048)
        .ok().and_then(|response| serde_json::from_str::<Value>(response["text"].as_str()?).ok())
        .and_then(|response| response["query"].as_str().map(str::trim).map(str::to_owned))
        .filter(|query| !query.is_empty() && query.len() <= 240 && urls(query).is_empty()
            && crate::privacy::classify(query).scope == "public");
    let original = search_query(question);
    let query = query.as_deref().unwrap_or(&original);
    match gather_pages(query) {
        Ok(pages) if pages.is_empty() && query != original => gather_pages(&original),
        result => result,
    }
}

pub fn gather(question: &str) -> Result<Vec<Value>, String> {
    search(question, false)
}

pub fn gather_pages(question: &str) -> Result<Vec<Value>, String> {
    search(question, true)
}

fn search(question: &str, pages_only: bool) -> Result<Vec<Value>, String> {
    let direct = urls(question);
    if !direct.is_empty() {
        return direct.iter().map(|url| fetch(url)).collect();
    }
    let base =
        std::env::var("FABRIC_SEARCH_URL").unwrap_or_else(|_| "http://127.0.0.1:8877".into());
    let mut url = Url::parse(&base).map_err(|_| "Invalid FABRIC_SEARCH_URL")?;
    let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost"));
    if !local && url.scheme() != "https" {
        return Err("Remote search service requires HTTPS".into());
    }
    url.set_path(&format!("{}/search", url.path().trim_end_matches('/')));
    let client = if local {
        Client::builder()
            .no_proxy()
            .timeout(crate::agent::bounded_timeout(25)?)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "Cannot initialize search")?
    } else {
        http(&url)?
    };
    let query=search_query(question);
    let response = client
        .post(url)
        .form(&[
            ("q", query.as_str()),
            ("format", "json"),
            ("categories", "general"),
        ])
        .send()
        .map_err(|_| "Web search is unavailable. Restart fabric-memory and check your internet connection.".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Search service returned HTTP {}",
            response.status().as_u16()
        ));
    }
    let value: Value =
        serde_json::from_str(&body(response)?).map_err(|_| "Search service did not return JSON")?;
    let results = value["results"].as_array().ok_or("Search service did not return a results list")?;
    let mut sources = Vec::new();
    let mut fetched = 0;
    for result in results.iter().take(6) {
        let Some(address) = result["url"].as_str() else {
            continue;
        };
        let Ok(mut url) = Url::parse(address) else {
            continue;
        };
        url.set_fragment(None);
        if resolve(&url).is_err() || sources.iter().any(|v: &Value| v["url"] == url.as_str()) {
            continue;
        }
        let snippet = plain(result["content"].as_str().unwrap_or(""), 1500);
        if !relevant(&query,result["title"].as_str().unwrap_or(""),&snippet){continue;}
        if fetched < if pages_only { 6 } else { 2 } {
            fetched += 1;
            if let Ok(page) = fetch(url.as_str()) {
                sources.push(page);
                if sources.len() == 3 {
                    break;
                }
                continue;
            }
        }
        if !pages_only && !snippet.is_empty() {
            sources.push(json!({"title":plain(result["title"].as_str().unwrap_or("Web result"),240),"url":url.as_str(),"content":snippet,"kind":"search snippet"}));
        }
        if sources.len() == 3 {
            break;
        }
    }
    Ok(sources)
}
/// Small local models can repeat old capability disclaimers despite fresh evidence.
/// Keep the UI truthful by falling back to actual retrieved results in that case.
pub fn denies_access(text: &str) -> bool {
    let text=text.to_lowercase().replace('’', "'");
    ["don't have live", "do not have live", "don't have direct access", "don't have direct google", "cannot browse", "don't have the ability to browse", "can't access live", "cannot access live", "can't browse", "can't retrieve live", "cannot retrieve live", "can't fetch", "cannot fetch", "can't check google", "cannot check google", "without a specific source", "no live internet", "lack live web"].iter().any(|phrase|text.contains(phrase)) || text.trim_start().starts_with("the user asks")
}
pub fn result_summary(sources: &[Value]) -> String {
    let mut text="Here are the live web results:\n\n".to_string();
    for (i,source) in sources.iter().enumerate() {
        let excerpt=source["content"].as_str().unwrap_or("").split_whitespace().take(40).collect::<Vec<_>>().join(" ");
        text.push_str(&format!("- **{}** — {}… [S{}]\n",source["title"].as_str().unwrap_or("Source"),excerpt,i+1));
    }
    text
}
pub fn context(sources: &[Value]) -> String {
    sources
        .iter()
        .enumerate()
        .map(|(i, s)| {
            format!(
                "[S{}] {}\nURL: {}\n{}",
                i + 1,
                s["title"].as_str().unwrap_or(""),
                s["url"].as_str().unwrap_or(""),
                s["content"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(1000)
                    .collect::<String>()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn news_queries_keep_topics_and_drop_command_words() {
        assert_eq!(search_query("Fetch and summarize the latest news"),"latest news");
        assert_eq!(search_query("check google for most recent news"),"latest news");
        assert_eq!(search_query("What is the latest news about Apple?"),"latest news about Apple");
    }
    #[test]
    fn live_results_replace_false_capability_refusals() {
        assert!(denies_access("I can’t retrieve live web content without a specific source or tool integration."));
        assert!(!denies_access("Here are today's headlines [S1]."));
        let result=result_summary(&[json!({"title":"Actual source","content":"A verified result from retrieval."})]);
        assert!(result.contains("Actual source"));
        assert!(result.contains("[S1]"));
        assert!(!denies_access(&result));
    }
    #[test]
    fn reader_blocks_internal_networks_and_redirect_targets() {
        for value in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "192.168.0.1",
            "::1",
            "::ffff:127.0.0.1",
            "fd00::1",
            "2001:db8::1",
        ] {
            assert!(!public(value.parse().unwrap()), "{value}");
        }
        assert!(public("1.1.1.1".parse().unwrap()));
        assert!(resolve(&Url::parse("http://127.0.0.1/secret").unwrap()).is_err());
        assert!(resolve(&Url::parse("https://name:secret@example.com/").unwrap()).is_err());
    }
    #[test]
    fn reader_omits_scripts_and_extracts_user_urls() {
        let (_,text)=page("<title>Example</title><main>Useful text<script>secret script</script><nav>menus</nav><p>More evidence</p></main>");
        assert!(text.contains("Useful text"));
        assert!(!text.contains("secret"));
        assert!(!text.contains("menus"));
        assert_eq!(
            urls("Read https://example.com/docs, please"),
            vec!["https://example.com/docs"]
        );
    }
}

#[cfg(test)]
mod relevance_tests {use super::*;
#[test]fn ignores_unrelated_single_word_hits(){assert!(!relevant("chanel brown leather bag","Brown University","University news"));assert!(!relevant("International Mathematical Olympiad 2025 problem 6","International Trucks","Heavy trucks"));assert!(relevant("chanel brown leather bag","Chanel brown leather flap bag","Handbag listing"));}
}


#[cfg(test)]
mod rating_evidence_tests {
    use super::*;
    #[test]
    fn extracts_a_published_rating_and_ignores_page_prose() {
        assert!(aggregate_rating("<p>Rating: 4.9 out of 5</p>").is_none());
        let html = r#"<script type="application/ld+json">
          {"@graph":[{"@type":"Recipe","aggregateRating":{"ratingValue":"9.2","bestRating":10}}]}
          </script>"#;
        let rating = aggregate_rating(html).unwrap();
        assert!((rating["out_of_five"].as_f64().unwrap() - 4.6).abs() < 1e-8);
    }
}
