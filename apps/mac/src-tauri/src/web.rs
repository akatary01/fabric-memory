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
        .timeout(Duration::from_secs(15))
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
        return Ok(json!({"title":title,"url":url.as_str(),"content":text,"kind":"page"}));
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
pub fn gather(question: &str) -> Result<Vec<Value>, String> {
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
            .timeout(Duration::from_secs(25))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "Cannot initialize search")?
    } else {
        http(&url)?
    };
    let response = client
        .post(url)
        .form(&[
            ("q", question),
            ("format", "json"),
            ("categories", "general"),
        ])
        .send()
        .map_err(|_| "Web search is unavailable. Run npm run web:setup.".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Search service returned HTTP {}",
            response.status().as_u16()
        ));
    }
    let value: Value =
        serde_json::from_str(&body(response)?).map_err(|_| "Search service did not return JSON")?;
    let mut sources = Vec::new();
    let mut fetched = 0;
    for result in value["results"].as_array().into_iter().flatten().take(6) {
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
        if fetched < 2 {
            fetched += 1;
            if let Ok(page) = fetch(url.as_str()) {
                sources.push(page);
                if sources.len() == 3 {
                    break;
                }
                continue;
            }
        }
        if !snippet.is_empty() {
            sources.push(json!({"title":plain(result["title"].as_str().unwrap_or("Web result"),240),"url":url.as_str(),"content":snippet,"kind":"search snippet"}));
        }
        if sources.len() == 3 {
            break;
        }
    }
    if sources.is_empty() {
        return Err(
            "No usable web sources were returned. Try a direct public URL or a different search."
                .into(),
        );
    }
    Ok(sources)
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
