use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use reqwest::header::{ACCEPT, CONTENT_TYPE, LOCATION};
use reqwest::redirect::Policy;
use url::{Host, Url};

use crate::AgentError;

/// Most characters of course text kept, for fetched and pasted sources alike.
pub const MAX_TEXT_CHARS: usize = 120_000;

/// Course source text, from a fetched page or pasted by the owner.
/// `source_url` is the final URL after redirects, `None` for pasted text.
#[derive(Debug, Clone, PartialEq)]
pub struct CourseText {
    pub source_url: Option<String>,
    pub text: String,
    pub truncated: bool,
}

const MAX_REDIRECTS: usize = 5;
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(15);
const BLOCK_TAGS: [&str; 22] = [
    "p",
    "div",
    "br",
    "li",
    "ul",
    "ol",
    "tr",
    "td",
    "th",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "section",
    "article",
    "header",
    "footer",
    "table",
    "pre",
    "blockquote",
];
const HIDDEN_TAGS: [&str; 5] = ["script", "style", "noscript", "template", "svg"];

/// Whether `ip` is a public unicast address a course fetch may reach.
/// Private, loopback, link-local, unspecified, multicast, broadcast, CGNAT,
/// documentation, reserved and unique-local ranges are not, nor IPv6 forms
/// that embed such an IPv4 address.
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    let blocked = a == 0
        || a == 10
        || a == 127
        || (a == 100 && (b & 0xc0) == 64)
        || (a == 169 && b == 254)
        || (a == 172 && (b & 0xf0) == 16)
        || (a == 192 && b == 0 && (c == 0 || c == 2))
        || (a == 192 && b == 168)
        || (a == 198 && (b & 0xfe) == 18)
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224;
    !blocked
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    let s = ip.segments();
    let embedded = |hi: u16, lo: u16| Ipv4Addr::from(((hi as u32) << 16) | lo as u32);
    if s[0] == 0x64 && s[1] == 0xff9b {
        return s[2..6] == [0, 0, 0, 0] && is_public_v4(embedded(s[6], s[7]));
    }
    if s[0] == 0x2002 {
        return is_public_v4(embedded(s[1], s[2]));
    }
    let global_unicast = (s[0] & 0xe000) == 0x2000;
    let teredo = s[0] == 0x2001 && s[1] == 0;
    let documentation = (s[0] == 0x2001 && s[1] == 0x0db8) || (s[0] & 0xfff0) == 0x3ff0;
    global_unicast && !teredo && !documentation
}

/// Fetch one public course page as text. `http`/`https` only, every hop's
/// address checked and pinned, at most 5 redirects, 15 s total, 2 MB body,
/// `text/html` or `text/plain` only. Errors are [`AgentError::Fetch`].
pub async fn fetch_course(url: &str) -> Result<CourseText, AgentError> {
    fetch_with_policy(url, is_public_ip).await
}

/// [`fetch_course`] with the address rule injected, so tests can reach a
/// loopback server while every other hop still gets checked.
async fn fetch_with_policy(url: &str, allow: fn(IpAddr) -> bool) -> Result<CourseText, AgentError> {
    tokio::time::timeout(TIMEOUT, follow(url, allow))
        .await
        .map_err(|_| AgentError::Fetch(format!("timed out after {} s", TIMEOUT.as_secs())))?
}

async fn follow(url: &str, allow: fn(IpAddr) -> bool) -> Result<CourseText, AgentError> {
    let fetch_err = |m: String| AgentError::Fetch(m);
    let mut url = Url::parse(url).map_err(|e| fetch_err(format!("bad url `{url}`: {e}")))?;
    for _ in 0..=MAX_REDIRECTS {
        let addr = pinned_addr(&url, allow).await?;
        let mut builder = reqwest::Client::builder()
            .redirect(Policy::none())
            .no_proxy()
            .timeout(TIMEOUT);
        if let Some(Host::Domain(domain)) = url.host() {
            builder = builder.resolve(domain, addr);
        }
        let client = builder.build().map_err(|e| fetch_err(e.to_string()))?;
        let mut response = client
            .get(url.clone())
            .header(ACCEPT, "text/html, text/plain")
            .send()
            .await
            .map_err(|e| fetch_err(format!("{url}: {e}")))?;
        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|l| l.to_str().ok())
                .ok_or_else(|| fetch_err(format!("{url}: redirect without a valid Location")))?;
            url = url
                .join(location)
                .map_err(|e| fetch_err(format!("bad redirect `{location}`: {e}")))?;
            continue;
        }
        if !status.is_success() {
            return Err(fetch_err(format!("{url}: HTTP {status}")));
        }
        let mime = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .map(|v| v.trim().to_ascii_lowercase())
            .unwrap_or_default();
        if mime != "text/html" && mime != "text/plain" {
            return Err(fetch_err(format!(
                "{url}: content type `{mime}` is not text/html or text/plain"
            )));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| fetch_err(format!("{url}: {e}")))?
        {
            let room = MAX_BODY_BYTES - body.len();
            body.extend_from_slice(&chunk[..chunk.len().min(room)]);
            if chunk.len() >= room {
                break;
            }
        }
        assert!(body.len() <= MAX_BODY_BYTES);
        let raw = String::from_utf8_lossy(&body);
        let text = if mime == "text/html" {
            html_to_text(&raw)
        } else {
            raw.into_owned()
        };
        return Ok(capped(Some(url.to_string()), &text));
    }
    Err(fetch_err(format!("more than {MAX_REDIRECTS} redirects")))
}

/// The one address to connect to for `url`, after checking the scheme and
/// that every address the host resolves to passes `allow`.
async fn pinned_addr(url: &Url, allow: fn(IpAddr) -> bool) -> Result<SocketAddr, AgentError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AgentError::Fetch(format!(
            "scheme `{}` is not http or https",
            url.scheme()
        )));
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| AgentError::Fetch(format!("{url}: no port")))?;
    let addrs: Vec<SocketAddr> = match url.host() {
        Some(Host::Ipv4(ip)) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
        Some(Host::Ipv6(ip)) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
        Some(Host::Domain(domain)) => tokio::net::lookup_host((domain, port))
            .await
            .map_err(|e| AgentError::Fetch(format!("{domain}: {e}")))?
            .collect(),
        None => return Err(AgentError::Fetch(format!("{url}: no host"))),
    };
    if let Some(bad) = addrs.iter().find(|a| !allow(a.ip())) {
        return Err(AgentError::Fetch(format!(
            "{url}: address {} is not public",
            bad.ip()
        )));
    }
    addrs
        .first()
        .copied()
        .ok_or_else(|| AgentError::Fetch(format!("{url}: host has no address")))
}

/// Visible text of `html`: tags removed, `script`, `style` and similar
/// dropped, block tags turned into line breaks, common entities decoded and
/// whitespace collapsed.
pub fn html_to_text(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len() / 2);
    let mut i = 0;
    while i < html.len() {
        let rest = &lower[i..];
        let starts_tag = rest.starts_with('<')
            && rest[1..]
                .starts_with(|c: char| c.is_ascii_alphabetic() || matches!(c, '/' | '!' | '?'));
        if !starts_tag {
            let first = rest.chars().next().map_or(1, char::len_utf8);
            let next = rest[first..].find('<').map_or(rest.len(), |n| n + first);
            out.push_str(&decode_entities(&html[i..i + next]));
            i += next;
            continue;
        }
        if rest.starts_with("<!--") {
            i += rest.find("-->").map_or(rest.len(), |e| e + 3);
            continue;
        }
        let end = rest.find('>').map_or(rest.len(), |e| e + 1);
        let name: String = rest[1..end]
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if !rest.starts_with("</") && HIDDEN_TAGS.contains(&name.as_str()) {
            let close = format!("</{name}");
            i += match rest.find(&close) {
                Some(c) => rest[c..].find('>').map_or(rest.len(), |e| c + e + 1),
                None => rest.len(),
            };
            continue;
        }
        if BLOCK_TAGS.contains(&name.as_str()) {
            out.push('\n');
        }
        i += end;
    }
    out.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `text` with `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`, `&#39;`,
/// `&nbsp;` and numeric entities decoded. Unknown entities stay as written.
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest[1..]
            .find(';')
            .filter(|&semi| semi <= 10)
            .and_then(|semi| {
                let entity = &rest[1..semi + 1];
                let ch = match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    _ => entity
                        .strip_prefix("#x")
                        .or_else(|| entity.strip_prefix("#X"))
                        .map(|hex| u32::from_str_radix(hex, 16))
                        .or_else(|| entity.strip_prefix('#').map(str::parse::<u32>))
                        .and_then(Result::ok)
                        .and_then(char::from_u32),
                };
                ch.map(|ch| (ch, semi + 2))
            });
        match decoded {
            Some((ch, len)) => {
                out.push(ch);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Owner-pasted or uploaded course text as a course source, capped like a
/// fetched page. Blank text is a `Validation` error.
pub fn pasted_text(text: &str) -> Result<CourseText, AgentError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(AgentError::Validation("pasted course text is empty".into()));
    }
    Ok(capped(None, text))
}

fn capped(source_url: Option<String>, text: &str) -> CourseText {
    let truncated = text.chars().nth(MAX_TEXT_CHARS).is_some();
    let text: String = if truncated {
        text.chars().take(MAX_TEXT_CHARS).collect()
    } else {
        text.to_owned()
    };
    CourseText {
        source_url,
        text,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn blocks_non_public_addresses() {
        for bad in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "0.0.0.0",
            "100.64.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "192.0.2.1",
            "198.51.100.7",
            "203.0.113.9",
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd12::1",
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "64:ff9b::a00:1",
        ] {
            assert!(!is_public_ip(ip(bad)), "{bad} must be blocked");
        }
        for good in [
            "8.8.8.8",
            "1.1.1.1",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(is_public_ip(ip(good)), "{good} must be allowed");
        }
    }

    #[tokio::test]
    async fn rejects_bad_schemes_and_private_hosts() {
        for url in [
            "file:///etc/passwd",
            "ftp://example.com/",
            "gopher://x/",
            "http://127.0.0.1/",
            "http://[::1]/",
            "http://10.0.0.1/",
            "http://localhost/",
        ] {
            let err = fetch_course(url).await.unwrap_err();
            assert!(matches!(err, AgentError::Fetch(_)), "{url}: {err}");
        }
    }

    fn test_policy(ip: IpAddr) -> bool {
        ip == IpAddr::V4(Ipv4Addr::LOCALHOST) || is_public_ip(ip)
    }

    async fn serve(responses: Vec<String>) -> (SocketAddr, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        tokio::spawn(async move {
            for body in responses {
                let (mut sock, _) = listener.accept().await.unwrap();
                counter.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                sock.write_all(body.as_bytes()).await.unwrap();
                sock.shutdown().await.ok();
            }
        });
        (addr, hits)
    }

    fn redirect(to: &str) -> String {
        format!(
            "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
    }

    #[tokio::test]
    async fn redirect_to_private_address_rejected_per_hop() {
        for target in [
            "http://10.0.0.1/x",
            "http://[::ffff:192.168.0.1]/",
            "http://169.254.169.254/latest",
        ] {
            let (addr, hits) = serve(vec![redirect(target)]).await;
            let err = fetch_with_policy(&format!("http://{addr}/start"), test_policy)
                .await
                .unwrap_err();
            assert_eq!(hits.load(Ordering::SeqCst), 1, "first hop must be served");
            assert!(
                matches!(err, AgentError::Fetch(ref m) if m.contains("not public")),
                "{target}: {err}"
            );
        }
    }

    #[tokio::test]
    async fn follows_public_redirect_and_strips_html() {
        let page = "<html><head><style>p{}</style><script>alert(1)</script></head><body><h1>Rust &amp; You</h1><p>Lesson 1</p></body></html>";
        let ok = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
            page.len()
        );
        let (addr, hits) = serve(vec![redirect("/page"), ok]).await;
        let got = fetch_with_policy(&format!("http://{addr}/start"), test_policy)
            .await
            .unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert!(
            got.text.contains("Rust & You") && got.text.contains("Lesson 1"),
            "{}",
            got.text
        );
        assert!(!got.text.contains("alert") && !got.text.contains("p{}"));
        assert!(got.source_url.unwrap().ends_with("/page"));
    }

    #[tokio::test]
    async fn too_many_redirects_rejected() {
        let (addr, _) = serve((0..7).map(|_| redirect("/again")).collect()).await;
        let err = fetch_with_policy(&format!("http://{addr}/"), test_policy)
            .await
            .unwrap_err();
        assert!(
            matches!(err, AgentError::Fetch(ref m) if m.contains("redirects")),
            "{err}"
        );
    }

    #[test]
    fn pasted_text_capped_and_non_empty() {
        assert!(matches!(pasted_text("   "), Err(AgentError::Validation(_))));
        let big = "a".repeat(MAX_TEXT_CHARS + 10);
        let out = pasted_text(&big).unwrap();
        assert!(out.truncated && out.text.chars().count() == MAX_TEXT_CHARS);
    }
}
