use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
    sync::OnceLock,
    time::Duration,
};

use dioxus::prelude::ServerFnError;
use url::Url;

use super::{internal, origin, request_error, unavailable};
use crate::preview::{PreviewCandidate, PreviewTarget};

pub(super) const CONNECT_TIMEOUT: Duration = Duration::from_millis(900);
pub(super) const TARGET_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const MAX_DISCOVERED_LISTENERS: usize = 64;
static HTTP_CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();

pub(super) fn validate_target(target: &PreviewTarget) -> Result<Url, ServerFnError> {
    let mut url = match target {
        PreviewTarget::Loopback { port } => {
            validate_port(*port)?;
            Url::parse(&format!("http://127.0.0.1:{port}/"))
                .map_err(|_| internal("Could not construct the loopback preview target."))?
        }
        PreviewTarget::Url { url } => {
            let parsed = Url::parse(url.trim())
                .map_err(|_| request_error("Enter a valid HTTP or HTTPS target URL.", 400))?;
            if !matches!(parsed.scheme(), "http" | "https")
                || parsed.host_str().is_none()
                || parsed.username() != ""
                || parsed.password().is_some()
                || parsed.query().is_some()
                || parsed.fragment().is_some()
                || parsed.path() != "/"
            {
                return Err(request_error(
                    "The target must be an HTTP(S) origin without credentials, path, query, or fragment.",
                    400,
                ));
            }
            parsed
        }
    };
    if target_is_loopback(&url)
        && std::env::var("PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            == url.port_or_known_default()
    {
        return Err(request_error(
            "The Syntaxis server cannot be used as a preview target.",
            400,
        ));
    }
    url.set_path("/");
    Ok(url)
}

fn validate_port(port: u16) -> Result<(), ServerFnError> {
    if port == 0 {
        return Err(request_error(
            "Preview ports must be between 1 and 65535.",
            400,
        ));
    }
    if std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        == Some(port)
    {
        return Err(request_error(
            "The Syntaxis server port cannot be used as a preview target.",
            400,
        ));
    }
    Ok(())
}

fn target_is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    }
}

pub(super) async fn resolve_loopback_target(
    client: &reqwest::Client,
    port: u16,
    timeout: Duration,
) -> Option<Url> {
    for host in ["127.0.0.1", "[::1]"] {
        let url = Url::parse(&format!("http://{host}:{port}/")).ok()?;
        let reachable = tokio::time::timeout(timeout, client.get(url.clone()).send())
            .await
            .ok()
            .and_then(Result::ok)
            .is_some();
        if reachable {
            return Some(url);
        }
    }
    None
}

pub(super) async fn probe_target(upstream: &Url) -> Result<(), ServerFnError> {
    let response = tokio::time::timeout(
        TARGET_PROBE_TIMEOUT,
        http_client()?.get(upstream.clone()).send(),
    )
    .await
    .map_err(|_| unavailable("The preview target did not respond in time."))?;
    response.map_err(|_| {
        unavailable(format!(
            "Could not connect to {} from the Syntaxis runtime.",
            target_label(upstream)
        ))
    })?;
    Ok(())
}

pub(super) fn target_label(upstream: &Url) -> String {
    origin(upstream).unwrap_or_else(|_| "unknown target".to_owned())
}

pub(super) fn discover_workspace_listeners(root: &Path) -> Result<Vec<PreviewCandidate>, String> {
    let root = root
        .canonicalize()
        .map_err(|_| "The workspace directory is unavailable.".to_owned())?;
    let mut sockets = HashMap::new();
    for path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        sockets.extend(parse_listening_sockets(&contents));
    }
    if sockets.is_empty() {
        return Ok(Vec::new());
    }

    let syntaxis_port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok());
    let mut listeners = BTreeMap::<u16, String>::new();
    let processes = fs::read_dir("/proc")
        .map_err(|_| "Runtime process inspection is unavailable.".to_owned())?;
    for process in processes.flatten() {
        let Some(pid) = process
            .file_name()
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let process_path = process.path();
        let Ok(cwd) = fs::read_link(process_path.join("cwd")) else {
            continue;
        };
        if !cwd.starts_with(&root) {
            continue;
        }
        let name = process_name(&process_path, pid);
        let Ok(files) = fs::read_dir(process_path.join("fd")) else {
            continue;
        };
        for file in files.flatten() {
            let Ok(target) = fs::read_link(file.path()) else {
                continue;
            };
            let Some(inode) = socket_inode(&target) else {
                continue;
            };
            let Some(port) = sockets.get(&inode).copied() else {
                continue;
            };
            if syntaxis_port == Some(port) {
                continue;
            }
            listeners.entry(port).or_insert_with(|| name.clone());
        }
    }
    Ok(listeners
        .into_iter()
        .map(|(port, process)| PreviewCandidate { port, process })
        .collect())
}

fn parse_listening_sockets(contents: &str) -> HashMap<u64, u16> {
    let mut sockets = HashMap::new();
    for line in contents.lines().skip(1) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 10 || fields[3] != "0A" {
            continue;
        }
        let Some((address, port)) = fields[1].split_once(':') else {
            continue;
        };
        if !is_loopback_or_unspecified(address) {
            continue;
        }
        let Ok(port) = u16::from_str_radix(port, 16) else {
            continue;
        };
        let Ok(inode) = fields[9].parse::<u64>() else {
            continue;
        };
        if port != 0 && inode != 0 {
            sockets.insert(inode, port);
        }
    }
    sockets
}

fn is_loopback_or_unspecified(address: &str) -> bool {
    match address.len() {
        8 => address == "00000000" || address.ends_with("00007F"),
        32 => {
            address == "00000000000000000000000000000000"
                || address == "00000000000000000000000001000000"
        }
        _ => false,
    }
}

fn socket_inode(target: &Path) -> Option<u64> {
    target
        .to_str()?
        .strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

fn process_name(process_path: &Path, pid: u32) -> String {
    fs::read_to_string(process_path.join("comm"))
        .ok()
        .map(|name| {
            name.trim()
                .chars()
                .filter(|character| !character.is_control())
                .take(48)
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("process {pid}"))
}

pub(super) fn http_client() -> Result<&'static reqwest::Client, ServerFnError> {
    HTTP_CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(3))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|error| format!("Could not create the preview HTTP client: {error}"))
        })
        .as_ref()
        .map_err(|error| internal(error.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn explicit_targets_must_be_http_origins() {
        assert_eq!(
            validate_target(&PreviewTarget::Url {
                url: "https://app.example.test".into(),
            })
            .unwrap()
            .as_str(),
            "https://app.example.test/"
        );
        assert_eq!(
            validate_target(&PreviewTarget::Url {
                url: "http://frontend:3000".into(),
            })
            .unwrap()
            .as_str(),
            "http://frontend:3000/"
        );

        for url in [
            "ftp://app.example.test",
            "https://user@app.example.test",
            "https://app.example.test/base",
            "https://app.example.test/?token=secret",
            "https://app.example.test/#fragment",
        ] {
            validate_target(&PreviewTarget::Url { url: url.into() }).unwrap_err();
        }
    }

    async fn serve_one_http_request(listener: tokio::net::TcpListener) {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 1_024];
        let _ = stream.read(&mut request).await.unwrap();
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
            .await
            .unwrap();
    }

    fn loopback_test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn loopback_targets_prefer_ipv4() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(serve_one_http_request(listener));

        let resolved =
            resolve_loopback_target(&loopback_test_client(), port, Duration::from_secs(1))
                .await
                .unwrap();

        assert_eq!(resolved.host_str(), Some("127.0.0.1"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn loopback_targets_fall_back_to_ipv6() {
        let listener = tokio::net::TcpListener::bind(("::1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(serve_one_http_request(listener));

        let resolved =
            resolve_loopback_target(&loopback_test_client(), port, Duration::from_secs(1))
                .await
                .unwrap();

        assert_eq!(resolved.host_str(), Some("[::1]"));
        server.await.unwrap();
    }

    #[test]
    fn proc_socket_parser_keeps_only_local_listeners() {
        let contents = "\
  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt uid timeout inode
   0: 0100007F:1435 00000000:0000 0A 00000000:00000000 00:00000000 00000000 1000 0 12345 1
   1: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000 1000 0 23456 1
   2: 0102A8C0:2382 00000000:0000 0A 00000000:00000000 00:00000000 00000000 1000 0 34567 1
   3: 0100007F:0CEA 00000000:0000 01 00000000:00000000 00:00000000 00000000 1000 0 45678 1
   4: 00000000000000000000000001000000:0BB8 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000 1000 0 56789 1
";

        let sockets = parse_listening_sockets(contents);

        assert_eq!(sockets.get(&12_345), Some(&5_173));
        assert_eq!(sockets.get(&23_456), Some(&8_080));
        assert_eq!(sockets.get(&56_789), Some(&3_000));
        assert!(!sockets.contains_key(&34_567));
        assert!(!sockets.contains_key(&45_678));
    }

    #[test]
    fn socket_links_expose_their_inode() {
        assert_eq!(socket_inode(Path::new("socket:[98765]")), Some(98_765));
        assert_eq!(socket_inode(Path::new("pipe:[98765]")), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn discovery_attributes_a_listener_to_the_workspace_process() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let root = std::env::current_dir().unwrap();

        let candidates = discover_workspace_listeners(&root).unwrap();

        assert!(candidates.iter().any(|candidate| candidate.port == port));
    }
}
