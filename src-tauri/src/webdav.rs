use crate::sync::{RemoteEntry, SyncRemote};
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::{
    collections::HashSet,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Characters that must be escaped inside a URL path segment. `/` is excluded
/// because segments are joined afterwards.
const PATH_SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'#')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'%')
    .add(b'^')
    .add(b'[')
    .add(b']')
    .add(b'|')
    .add(b'\\');

pub struct WebDavRemote {
    /// Normalized to always end with `/`, so joining is unambiguous.
    base: String,
    username: String,
    password: String,
    agent: ureq::Agent,
}

impl WebDavRemote {
    pub fn new(url: &str, username: &str, password: &str) -> Result<Self, String> {
        let trimmed = url.trim().trim_end_matches('/');
        if trimmed.is_empty() {
            return Err("WebDAV URL cannot be empty.".to_string());
        }
        if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
            return Err("WebDAV URL must start with http:// or https://".to_string());
        }

        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            .timeout(Duration::from_secs(60))
            .build();

        Ok(Self {
            base: format!("{trimmed}/"),
            username: username.to_string(),
            password: password.to_string(),
            agent,
        })
    }

    fn encode_path(rel: &str) -> String {
        rel.split('/')
            .filter(|s| !s.is_empty() && *s != "." && *s != "..")
            .map(|s| utf8_percent_encode(s, PATH_SEGMENT).to_string())
            .collect::<Vec<_>>()
            .join("/")
    }

    fn url_for(&self, rel: &str) -> String {
        format!("{}{}", self.base, Self::encode_path(rel))
    }

    fn request(&self, method: &str, url: &str) -> ureq::Request {
        let req = self.agent.request(method, url);
        if self.username.is_empty() {
            req
        } else {
            // Basic auth over http:// would send this in the clear; the URL
            // scheme check in `new` allows it deliberately for LAN servers, but
            // that's the user's call to make.
            req.set(
                "Authorization",
                &format!("Basic {}", base64(&format!("{}:{}", self.username, self.password))),
            )
        }
    }

    /// The base URL's path portion, used to turn absolute hrefs from the server
    /// back into vault-relative paths.
    fn base_path(&self) -> String {
        let without_scheme = self
            .base
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(&self.base);
        match without_scheme.find('/') {
            Some(i) => without_scheme[i..].to_string(),
            None => "/".to_string(),
        }
    }

    /// One level of PROPFIND. Depth: infinity is refused by many servers
    /// (Apache and Nextcloud disable it), so listing recurses a level at a time.
    fn propfind(&self, rel: &str) -> Result<Vec<(String, bool)>, String> {
        let url = if rel.is_empty() {
            self.base.clone()
        } else {
            format!("{}/", self.url_for(rel).trim_end_matches('/'))
        };

        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/><d:getlastmodified/></d:prop></d:propfind>"#;

        let response = self
            .request("PROPFIND", &url)
            .set("Depth", "1")
            .set("Content-Type", "application/xml; charset=utf-8")
            .send_string(body)
            .map_err(|e| describe(e, &url))?;

        let text = response
            .into_string()
            .map_err(|e| format!("Failed to read listing: {e}"))?;

        parse_multistatus(&text, &self.base_path())
    }

    /// Creates a collection and every missing parent. 405 means it already
    /// exists, which is success for our purposes.
    fn mkcol_p(&self, rel: &str) -> Result<(), String> {
        let mut prefix = String::new();
        for segment in rel.split('/').filter(|s| !s.is_empty()) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(segment);

            let url = format!("{}/", self.url_for(&prefix).trim_end_matches('/'));
            match self.request("MKCOL", &url).call() {
                Ok(_) => {}
                Err(ureq::Error::Status(405 | 301, _)) => {}
                Err(e) => return Err(describe(e, &url)),
            }
        }
        Ok(())
    }
}

/// Minimal base64 for the Authorization header — not worth a dependency.
fn base64(input: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::new();

    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Turns transport errors into something a user can act on. A bare
/// "status code 401" doesn't tell anyone what to change.
fn describe(e: ureq::Error, url: &str) -> String {
    match e {
        ureq::Error::Status(401, _) => "Authentication failed — check the username and password.".to_string(),
        ureq::Error::Status(403, _) => "The server refused access to that path.".to_string(),
        ureq::Error::Status(404, _) => format!("Not found on the server: {url}"),
        ureq::Error::Status(507, _) => "The server is out of storage.".to_string(),
        ureq::Error::Status(code, _) => format!("Server returned {code}."),
        ureq::Error::Transport(t) => format!("Couldn't reach the server: {t}"),
    }
}

/// Extracts `(relative path, is_collection)` from a PROPFIND multistatus body.
///
/// Namespace prefixes vary by server (`d:`, `D:`, `lp1:`), so elements are
/// matched on local name only.
fn parse_multistatus(xml: &str, base_path: &str) -> Result<Vec<(String, bool)>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut out = Vec::new();
    let mut buf = Vec::new();

    let mut in_href = false;
    let mut current_href: Option<String> = None;
    let mut is_collection = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match local_name(e.name().as_ref()) {
                b"href" => in_href = true,
                b"collection" => is_collection = true,
                b"response" => {
                    current_href = None;
                    is_collection = false;
                }
                _ => {}
            },
            Ok(Event::Empty(e)) => {
                if local_name(e.name().as_ref()) == b"collection" {
                    is_collection = true;
                }
            }
            Ok(Event::Text(e)) => {
                if in_href {
                    current_href = Some(e.unescape().unwrap_or_default().to_string());
                }
            }
            Ok(Event::End(e)) => match local_name(e.name().as_ref()) {
                b"href" => in_href = false,
                b"response" => {
                    if let Some(href) = current_href.take() {
                        if let Some(rel) = relative_from_href(&href, base_path) {
                            out.push((rel, is_collection));
                        }
                    }
                    is_collection = false;
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("Malformed server response: {e}")),
            _ => {}
        }
        buf.clear();
    }

    Ok(out)
}

fn local_name(qname: &[u8]) -> &[u8] {
    match qname.iter().position(|b| *b == b':') {
        Some(i) => &qname[i + 1..],
        None => qname,
    }
}

/// Strips the base path (and any scheme/host the server chose to include) off
/// an href, yielding a vault-relative path.
fn relative_from_href(href: &str, base_path: &str) -> Option<String> {
    let path = match href.find("://") {
        Some(i) => {
            let rest = &href[i + 3..];
            let slash = rest.find('/')?;
            &rest[slash..]
        }
        None => href,
    };

    let decoded = percent_decode_str(path).decode_utf8().ok()?.to_string();
    let rel = decoded.strip_prefix(base_path).unwrap_or(&decoded);
    let rel = rel.trim_matches('/');

    if rel.is_empty() {
        None
    } else {
        Some(rel.to_string())
    }
}

impl SyncRemote for WebDavRemote {
    fn list(&self) -> Result<Vec<RemoteEntry>, String> {
        let mut files = Vec::new();
        let mut queue = vec![String::new()];
        let mut visited: HashSet<String> = HashSet::new();

        while let Some(dir) = queue.pop() {
            if !visited.insert(dir.clone()) {
                continue;
            }

            let entries = match self.propfind(&dir) {
                Ok(e) => e,
                // A missing subdirectory mid-walk isn't fatal; the vault may
                // simply not have been created on the server yet.
                Err(_) if !dir.is_empty() => continue,
                Err(e) => return Err(e),
            };

            for (rel, is_dir) in entries {
                if rel == dir {
                    continue; // PROPFIND echoes the requested collection back
                }
                let name = rel.rsplit('/').next().unwrap_or("");
                if name.starts_with('.') && !rel.starts_with(".notemanager") {
                    continue;
                }

                if is_dir {
                    queue.push(rel);
                } else {
                    files.push(RemoteEntry {
                        path: rel,
                        mtime: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0),
                    });
                }
            }
        }

        Ok(files)
    }

    fn get(&self, path: &str) -> Result<Vec<u8>, String> {
        let url = self.url_for(path);
        let response = self
            .request("GET", &url)
            .call()
            .map_err(|e| describe(e, &url))?;

        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut response.into_reader(), &mut buf)
            .map_err(|e| format!("Failed to download {path}: {e}"))?;
        Ok(buf)
    }

    fn put(&self, path: &str, data: &[u8]) -> Result<(), String> {
        if let Some((parent, _)) = path.rsplit_once('/') {
            self.mkcol_p(parent)?;
        }
        let url = self.url_for(path);
        self.request("PUT", &url)
            .set("Content-Type", "text/markdown; charset=utf-8")
            .send_bytes(data)
            .map(|_| ())
            .map_err(|e| describe(e, &url))
    }

    fn delete(&self, path: &str) -> Result<(), String> {
        let url = self.url_for(path);
        match self.request("DELETE", &url).call() {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(404, _)) => Ok(()),
            Err(e) => Err(describe(e, &url)),
        }
    }

    fn id(&self) -> String {
        format!("webdav:{}", self.base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_spaces_and_unicode_in_paths() {
        assert_eq!(
            WebDavRemote::encode_path("My Notes/note_1.md"),
            "My%20Notes/note_1.md"
        );
        assert!(WebDavRemote::encode_path("Café/x.md").starts_with("Caf"));
    }

    #[test]
    fn drops_traversal_segments_when_encoding() {
        assert_eq!(WebDavRemote::encode_path("../../etc/x.md"), "etc/x.md");
    }

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64("user:pass"), "dXNlcjpwYXNz");
        assert_eq!(base64("a"), "YQ==");
        assert_eq!(base64("ab"), "YWI=");
        assert_eq!(base64("abc"), "YWJj");
    }

    #[test]
    fn relative_paths_survive_absolute_and_encoded_hrefs() {
        assert_eq!(
            relative_from_href("/dav/notes/My%20Notes/a.md", "/dav/notes/"),
            Some("My Notes/a.md".to_string())
        );
        assert_eq!(
            relative_from_href("https://host/dav/notes/a.md", "/dav/notes/"),
            Some("a.md".to_string())
        );
        assert_eq!(relative_from_href("/dav/notes/", "/dav/notes/"), None);
    }

    #[test]
    fn parses_a_multistatus_regardless_of_namespace_prefix() {
        let xml = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:">
  <D:response><D:href>/dav/</D:href>
    <D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop></D:propstat>
  </D:response>
  <D:response><D:href>/dav/General/</D:href>
    <D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop></D:propstat>
  </D:response>
  <D:response><D:href>/dav/General/note_1.md</D:href>
    <D:propstat><D:prop><D:resourcetype/></D:prop></D:propstat>
  </D:response>
</D:multistatus>"#;

        let parsed = parse_multistatus(xml, "/dav/").unwrap();
        assert_eq!(parsed.len(), 2); // the base collection itself yields None
        assert!(parsed.contains(&("General".to_string(), true)));
        assert!(parsed.contains(&("General/note_1.md".to_string(), false)));
    }

    #[test]
    fn rejects_non_http_urls() {
        assert!(WebDavRemote::new("ftp://example.com", "", "").is_err());
        assert!(WebDavRemote::new("", "", "").is_err());
        assert!(WebDavRemote::new("https://example.com/dav", "u", "p").is_ok());
    }
}
