//! Host-side loopback HTTP/CONNECT proxy enforcing manifest `network_domains` allowlists.

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

const MAX_PROXY_HEADER_BYTES: usize = 64 * 1024;

/// Ephemeral Host-owned HTTP/CONNECT proxy pinned to `127.0.0.1:<port>`.
///
/// Confined Tool subprocesses run with Seatbelt network outbound restricted exclusively
/// to this loopback port and `HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY` pointed here,
/// allowing outbound connections only to exact manifest `network_domains`.
pub(super) struct NetworkDomainProxy {
    port: u16,
    accept_task: JoinHandle<()>,
}

impl NetworkDomainProxy {
    pub(super) async fn start(allowed_domains: Vec<String>) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("bind loopback network domain proxy")?;
        let port = listener
            .local_addr()
            .context("read loopback network domain proxy port")?
            .port();
        let allowed = Arc::new(allowed_domains);
        let accept_task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let allowed = Arc::clone(&allowed);
                tokio::spawn(async move {
                    let _ = handle_proxy_connection(stream, &allowed).await;
                });
            }
        });
        Ok(Self { port, accept_task })
    }

    pub(super) const fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for NetworkDomainProxy {
    fn drop(&mut self) {
        self.accept_task.abort();
    }
}

pub(super) fn is_domain_allowed(host: &str, allowed_domains: &[String]) -> bool {
    let normalized = host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.');
    if normalized.is_empty() {
        return false;
    }
    allowed_domains
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(normalized))
}

async fn handle_proxy_connection(mut client: TcpStream, allowed_domains: &[String]) -> Result<()> {
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(pos) = find_header_end(&buf) {
            break pos;
        }
        if buf.len() >= MAX_PROXY_HEADER_BYTES {
            write_simple_response(
                &mut client,
                "431 Request Header Fields Too Large",
                Some("header-too-large"),
            )
            .await;
            bail!("proxy request headers exceeded limit");
        }
        let n = client.read(&mut chunk).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
    };

    let head_bytes = &buf[..header_end];
    let body_prefix = &buf[header_end + 4..];
    let head = std::str::from_utf8(head_bytes).context("proxy request is not UTF-8")?;
    let mut lines = head.split("\r\n");
    let request_line = lines.next().context("missing HTTP request line")?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().context("missing HTTP method")?;
    let target = parts.next().context("missing HTTP request target")?;
    let version = parts.next().unwrap_or("HTTP/1.1");

    if method.eq_ignore_ascii_case("CONNECT") {
        let Some((host, port)) = parse_authority(target, 443) else {
            write_simple_response(&mut client, "400 Bad Request", None).await;
            return Ok(());
        };
        if !is_domain_allowed(&host, allowed_domains) {
            write_simple_response(&mut client, "403 Forbidden", Some("domain-denied")).await;
            return Ok(());
        }
        let Ok(mut upstream) = TcpStream::connect((host.as_str(), port)).await else {
            write_simple_response(&mut client, "502 Bad Gateway", None).await;
            return Ok(());
        };
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
        if !body_prefix.is_empty() {
            upstream.write_all(body_prefix).await?;
        }
        let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
        return Ok(());
    }

    let Ok(url) = reqwest::Url::parse(target) else {
        write_simple_response(&mut client, "400 Bad Request", None).await;
        return Ok(());
    };
    if url.scheme() != "http" {
        write_simple_response(&mut client, "400 Bad Request", None).await;
        return Ok(());
    }
    let Some(host) = url.host_str() else {
        write_simple_response(&mut client, "400 Bad Request", None).await;
        return Ok(());
    };
    if !is_domain_allowed(host, allowed_domains) {
        write_simple_response(&mut client, "403 Forbidden", Some("domain-denied")).await;
        return Ok(());
    }
    let port = url.port_or_known_default().unwrap_or(80);
    let Ok(mut upstream) = TcpStream::connect((host, port)).await else {
        write_simple_response(&mut client, "502 Bad Gateway", None).await;
        return Ok(());
    };

    let mut path_and_query = url.path().to_string();
    if path_and_query.is_empty() {
        path_and_query.push('/');
    }
    if let Some(query) = url.query() {
        path_and_query.push('?');
        path_and_query.push_str(query);
    }

    let mut outbound = format!("{method} {path_and_query} {version}\r\n");
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((name, _)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("proxy-connection")
        {
            continue;
        }
        outbound.push_str(line);
        outbound.push_str("\r\n");
    }
    outbound.push_str("\r\n");

    upstream.write_all(outbound.as_bytes()).await?;
    if !body_prefix.is_empty() {
        upstream.write_all(body_prefix).await?;
    }
    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
    Ok(())
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_authority(authority: &str, default_port: u16) -> Option<(String, u16)> {
    let trimmed = authority.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        let port = match after.strip_prefix(':') {
            Some(p) => p.parse::<u16>().ok()?,
            None if after.is_empty() => default_port,
            None => return None,
        };
        return Some((host.to_string(), port));
    }
    match trimmed.rsplit_once(':') {
        Some((host, port_str)) if !host.is_empty() => {
            let port = port_str.parse::<u16>().ok()?;
            Some((host.to_string(), port))
        }
        _ => Some((trimmed.to_string(), default_port)),
    }
}

async fn write_simple_response(client: &mut TcpStream, status: &str, policy: Option<&str>) {
    let policy_header = policy
        .map(|val| format!("X-Alan-Network-Policy: {val}\r\n"))
        .unwrap_or_default();
    let response = format!(
        "HTTP/1.1 {status}\r\n{policy_header}Content-Length: 0\r\nConnection: close\r\n\r\n"
    );
    let _ = client.write_all(response.as_bytes()).await;
    let _ = client.shutdown().await;
}
