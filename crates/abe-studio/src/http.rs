//! The little HTTP/1.1 the studio host needs: read one request (bounded), write one response,
//! serve files from a folder without ever leaving it.
use std::io::{BufRead, Read, Write};
use std::path::{Component, Path, PathBuf};

/// Longest request line or header line, and most headers.
const MAX_LINE: usize = 8 * 1024;
const MAX_HEADERS: usize = 64;
/// Largest request body. The host takes no uploads (source assets go into the project folder,
/// not through here), so a body is small; it is read as it arrives, never allocated up front.
pub const MAX_BODY: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: String,
    /// The path without the query, percent-decoded.
    pub path: String,
    /// The raw query string, without `?`.
    pub query: String,
    /// Header names in lower case.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// The first header called `name` (in lower case).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    /// Every header called `name` (in lower case), in order.
    pub fn headers_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.headers
            .iter()
            .filter(move |(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    /// The value of `key` in the query string, percent-decoded.
    pub fn param(&self, key: &str) -> Option<String> {
        self.query.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (percent_decode(k).as_deref() == Some(key)).then(|| percent_decode(v))?
        })
    }
    /// The value of cookie `name`: the first pair in the `Cookie` headers that names it.
    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.headers_named("cookie")
            .flat_map(|v| v.split(';'))
            .find_map(|pair| {
                let (k, v) = pair.split_once('=')?;
                (k.trim() == name).then(|| v.trim())
            })
    }
}

fn read_line(r: &mut impl BufRead) -> Result<String, String> {
    let mut line = Vec::new();
    let n = r
        .by_ref()
        .take(MAX_LINE as u64 + 1)
        .read_until(b'\n', &mut line)
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("connection closed".into());
    }
    if line.len() > MAX_LINE || !line.ends_with(b"\n") {
        return Err("header line too long".into());
    }
    while line.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
        line.pop();
    }
    String::from_utf8(line).map_err(|_| "header is not UTF-8".into())
}

/// Decodes a query component: `%xx` escapes, and `+` as a space. `None` for a malformed escape
/// or text that is not UTF-8.
pub fn percent_decode(s: &str) -> Option<String> {
    decode(s, true)
}

/// Decodes a URL path: `%xx` escapes only, since `+` in a path is a plus sign (a file named
/// `day+night.look.json` is fetched as written). `None` for a malformed escape or non-UTF-8.
pub fn path_decode(s: &str) -> Option<String> {
    decode(s, false)
}

fn decode(s: &str, plus_is_space: bool) -> Option<String> {
    let b = s.as_bytes();
    let hex = |i: usize| b.get(i).and_then(|c| char::from(*c).to_digit(16));
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                out.push(u8::try_from(hex(i + 1)? * 16 + hex(i + 2)?).ok()?);
                i += 3;
            }
            b'+' if plus_is_space => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// Reads one request; `Ok(None)` when the peer closed the connection before sending one.
pub fn read_request(r: &mut impl BufRead) -> Result<Option<Request>, String> {
    let line = match read_line(r) {
        Ok(l) => l,
        Err(e) if e == "connection closed" => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next())
    else {
        return Err(format!("bad request line {line:?}"));
    };
    if !version.starts_with("HTTP/1.") {
        return Err(format!("unsupported protocol {version:?}"));
    }
    let (raw_path, query) = target.split_once('?').unwrap_or((target, ""));
    let path = path_decode(raw_path).ok_or("bad path encoding")?;
    let mut headers = vec![];
    loop {
        let h = read_line(r)?;
        if h.is_empty() {
            break;
        }
        if headers.len() >= MAX_HEADERS {
            return Err("too many headers".into());
        }
        let (k, v) = h.split_once(':').ok_or("bad header")?;
        headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
    }
    let mut req = Request {
        method: method.to_string(),
        path,
        query: query.to_string(),
        headers,
        body: vec![],
    };
    if req.header("transfer-encoding").is_some() {
        return Err("chunked request bodies are not accepted".into());
    }
    if let Some(len) = req.header("content-length") {
        let len: usize = len.parse().map_err(|_| "bad content-length")?;
        if len > MAX_BODY {
            return Err("request body too large".into());
        }
        r.by_ref()
            .take(len as u64)
            .read_to_end(&mut req.body)
            .map_err(|e| e.to_string())?;
        if req.body.len() != len {
            return Err("request body cut short".into());
        }
    }
    Ok(Some(req))
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".into(), content_type.into())],
            body,
        }
    }
    pub fn text(status: u16, text: &str) -> Self {
        Self::new(
            status,
            "text/plain; charset=utf-8",
            text.as_bytes().to_vec(),
        )
    }
    pub fn json(status: u16, text: String) -> Self {
        Self::new(status, "application/json; charset=utf-8", text.into_bytes())
    }
    pub fn header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.into(), v.into()));
        self
    }
    pub fn write_to(&self, w: &mut impl Write) -> std::io::Result<()> {
        let reason = match self.status {
            200 => "OK",
            101 => "Switching Protocols",
            204 => "No Content",
            304 => "Not Modified",
            400 => "Bad Request",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            413 => "Payload Too Large",
            429 => "Too Many Requests",
            _ => "Error",
        };
        let mut head = format!("HTTP/1.1 {} {reason}\r\n", self.status);
        for (k, v) in &self.headers {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
        if self.status != 101 {
            head.push_str(&format!("Content-Length: {}\r\n", self.body.len()));
            head.push_str("Cache-Control: no-store\r\n");
        }
        head.push_str("\r\n");
        w.write_all(head.as_bytes())?;
        w.write_all(&self.body)?;
        w.flush()
    }
}

pub fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("glb") => "model/gltf-binary",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// `root` joined with a request path, or `None` if the path would leave `root` (`..`, absolute
/// parts, drive prefixes) or names a hidden file.
pub fn safe_join(root: &Path, request_path: &str) -> Option<PathBuf> {
    let mut out = root.to_path_buf();
    for part in Path::new(request_path.trim_start_matches('/')).components() {
        match part {
            Component::Normal(p) => {
                if p.to_str()?.starts_with('.') {
                    return None;
                }
                out.push(p)
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

/// Serves a file below `root`, or `index.html` for a folder. Only the request path is checked:
/// this is for the host's own folders (the editor, the engine's runtime), which the person
/// running it chose.
pub fn serve_file(root: &Path, request_path: &str) -> Response {
    let Some(mut path) = safe_join(root, request_path) else {
        return Response::text(403, "forbidden");
    };
    if path.is_dir() {
        path.push("index.html");
    }
    match std::fs::read(&path) {
        Ok(bytes) => Response::new(200, content_type(&path), bytes),
        Err(_) => Response::text(404, "not found"),
    }
}

/// Serves a file below `dir`, a folder of the project whose own folder, with every symlink
/// resolved, is `real_root`. Wherever symlinks lead, the file must resolve inside `dir` and
/// inside the project, and not into a hidden folder there.
pub fn serve_inside(dir: &Path, request_path: &str, real_root: &Path) -> Response {
    let Some(path) = safe_join(dir, request_path) else {
        return Response::text(403, "forbidden");
    };
    let (Ok(real_dir), Ok(real)) = (std::fs::canonicalize(dir), std::fs::canonicalize(&path))
    else {
        return Response::text(404, "not found");
    };
    match real.strip_prefix(&real_dir) {
        Ok(rel) if real_dir.starts_with(real_root) && !hidden(rel) => {}
        _ => return Response::text(403, "forbidden: it leads outside the project"),
    }
    match std::fs::read(&real) {
        Ok(bytes) => Response::new(200, content_type(&real), bytes),
        Err(_) => Response::text(404, "not found"),
    }
}

/// Whether a relative path has a part that is hidden (starts with `.`), or is not a plain name.
pub fn hidden(rel: &Path) -> bool {
    rel.components().any(|c| match c {
        Component::Normal(p) => p.to_str().is_none_or(|n| n.starts_with('.')),
        _ => true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_parse_with_headers_query_and_body() {
        let raw = b"POST /api/doc%20x?id=ab%2Fc&flag HTTP/1.1\r\nHost: x\r\nContent-Length: 5\r\nUpgrade: websocket\r\n\r\nhello";
        let mut r = &raw[..];
        let req = read_request(&mut r).unwrap().unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/api/doc x");
        assert_eq!(req.param("id").as_deref(), Some("ab/c"));
        assert_eq!(req.param("flag").as_deref(), Some(""));
        assert_eq!(req.param("none"), None);
        assert_eq!(req.header("upgrade"), Some("websocket"));
        assert_eq!(req.body, b"hello");
        let mut empty = &b""[..];
        assert_eq!(read_request(&mut empty).unwrap(), None);
    }

    #[test]
    fn hostile_requests_are_refused() {
        let long = format!("GET /{} HTTP/1.1\r\n\r\n", "a".repeat(MAX_LINE));
        assert!(read_request(&mut long.as_bytes()).is_err());
        let big = format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        );
        assert!(read_request(&mut big.as_bytes()).is_err());
        assert!(read_request(&mut &b"GET / SPDY/3\r\n\r\n"[..]).is_err());
        let chunked = b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n";
        assert!(read_request(&mut &chunked[..]).is_err());
        // A body that is announced but never comes is an error, not a buffer of zeros.
        let short = b"POST / HTTP/1.1\r\nContent-Length: 10\r\n\r\nabc";
        assert!(read_request(&mut &short[..]).is_err());
    }

    #[test]
    fn a_plus_is_a_plus_in_paths_and_a_space_in_queries() {
        let raw = b"GET /project/day+night%20x.look.json?q=a+b%2Bc HTTP/1.1\r\n\r\n";
        let req = read_request(&mut &raw[..]).unwrap().unwrap();
        assert_eq!(req.path, "/project/day+night x.look.json");
        assert_eq!(req.param("q").as_deref(), Some("a b+c"));
        for bad in ["%", "%4", "%zz", "%+5", "%-5", "%ff"] {
            assert_eq!(path_decode(bad), None, "{bad}");
        }
        assert_eq!(path_decode("%41%2b"), Some("A+".into()));
    }

    #[test]
    fn cookies_are_found_by_name() {
        let raw = b"GET / HTTP/1.1\r\nCookie: a=1; abe-studio-7480=tok ; b=\r\nCookie: abe-studio-7480=later\r\n\r\n";
        let req = read_request(&mut &raw[..]).unwrap().unwrap();
        assert_eq!(req.cookie("abe-studio-7480"), Some("tok"));
        assert_eq!(req.cookie("b"), Some(""));
        assert_eq!(req.cookie("abe-studio"), None);
    }

    #[test]
    fn paths_never_leave_the_root() {
        let root = Path::new("/srv/editor");
        assert_eq!(
            safe_join(root, "/js/app.js"),
            Some(PathBuf::from("/srv/editor/js/app.js"))
        );
        for bad in ["/../secret", "/a/../../b", "/.git/config", "/a/.env"] {
            assert_eq!(safe_join(root, bad), None, "{bad}");
        }
    }

    #[test]
    fn responses_carry_length_and_status() {
        let mut out = vec![];
        Response::json(200, "{}".into())
            .header("X-Test", "1")
            .write_to(&mut out)
            .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Content-Length: 2\r\n"));
        assert!(text.contains("X-Test: 1\r\n"));
        assert!(text.ends_with("\r\n\r\n{}"));
    }
}
