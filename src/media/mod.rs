use crossbeam_channel::Sender;
use serde_json::Value;

// Module datastructre is self-contained for media handling
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaTrackInfo {
    pub title: String,
    pub artist: String,
    pub album: String, 
    pub is_playing: bool,
    pub source_app: String,
    pub album_art: Option<Vec<u8>>,
}

/// Cleans up track titles by removing common "garbage" suffixes often found in
/// metadata from sources like YouTube or streaming services (e.g., "(Official Video)").
/// This improves the accuracy of search queries (like Wikipedia lookups).
fn sanitize_title(title: &str) -> String {
    // List of terms that usually indicate the end of the actual song title
    let garbage_terms = [
        "(Official Video)",
        "(Official Music Video)",
        "(Lyric Video)",
        "(Audio)",
        "[Official Video]",
        "[Official Music Video]",
        "[Lyric Video]",
        "[Audio]",
        "ft.",
        "feat.",
        "featuring",
    ];
    let mut clean = title.to_string();
    for term in garbage_terms.iter() {
        // If any garbage term is found, truncate the string at that point
        if let Some(idx) = clean.to_lowercase().find(&term.to_lowercase()) {
            clean.truncate(idx);
        }
    }
    clean.trim().to_string()
}

/// Robustly encodes a string for use in a URL query parameter.
///
/// This serves as a defense against URL parameter injection. By percent-encoding
/// reserved characters (like '&', '=', '?'), we ensure that user input—such as
/// a song title containing these symbols—cannot break out of its data context
/// and alter the structure of the query string.
fn url_encode(input: &str) -> String {
    // We use a strict allow-list approach (RFC 3986) to guarantee safety without external dependencies.
    // Any character not explicitly whitelisted as "unreserved" is percent-encoded.
    // This neutralizes control characters that could otherwise be interpreted by the server.
    let mut encoded = String::with_capacity(input.len() * 2);
    for byte in input.bytes() {
        match byte {
            // Unreserved characters (RFC 3986)
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            // Space becomes '+'
            b' ' => encoded.push('+'),
            // Everything else gets percent-encoded
            _ => encoded.push_str(&format!("%{:02X}", byte)),
        }
    }
    encoded
}

/// Attempts to find a Wikipedia article URL for the given track.
///
/// It performs a search using the MediaWiki API.
/// - If an album is present, it searches for "Artist Album".
/// - Otherwise, it searches for "Artist Title".
///
/// Returns a direct link to the article if found, or a search results page URL as a fallback.
pub fn fetch_wikipedia_url(artist: &str, title: &str, album: &str) -> String {
    let clean_title = sanitize_title(title);
    
    // 1. Construct the Search Query
    // We prioritize "Artist Album" for better context, falling back to "Artist Title".
    let raw_query = if !album.is_empty() && album != "Unknown Album" {
        format!("{} {}", artist, album)
    } else {
        format!("{} {}", artist, clean_title)
    };

    // 2. Prepare for Fallback URL (Manual Encoding)
    // We manually encode the query here to prevent injection attacks in the fallback URL.
    // This ensures characters like '&' or '?' in song titles don't break the search link.
    let encoded_query = url_encode(&raw_query);

    let api_url = "https://en.wikipedia.org/w/api.php";
    
    // 3. Perform API Request (Automatic Encoding)
    // We pass the raw query to ureq, which handles URL encoding internally.
    // This prevents double-encoding issues while keeping the API request safe.
    let resp = ureq::get(api_url)
        .query("action", "query")
        .query("list", "search")
        .query("srsearch", &raw_query)
        .query("srlimit", "1")
        .query("format", "json")
        .call();

    match resp {
        Ok(response) => {
            if let Ok(json) = serde_json::from_reader::<_, Value>(response.into_reader()) {
                // 4. Parse the Result
                // Path: query -> search -> [0] -> title
                if let Some(title) = json.get("query")
                    .and_then(|q| q.get("search"))
                    .and_then(|s| s.get(0)) 
                    .and_then(|r| r.get("title"))
                    .and_then(|t| t.as_str()) 
                {
                    // 5. Construct Article URL
                    // Wikipedia slugs use underscores for spaces. We encode other chars
                    // to ensure the URL is valid and safe.
                    let url_slug = url_encode(title).replace("+", "_");
                    return format!("https://en.wikipedia.org/wiki/{}", url_slug);
                }
            }
        },
        Err(e) => {
            tracing::warn!("Wikipedia API request failed: {}", e);
        }
    }

    // 6. Fallback: Generic Search
    // If the API fails or finds nothing, return a search page URL.
    // We use the pre-encoded query string here to ensure safety.
    format!("https://en.wikipedia.org/w/index.php?search={}", encoded_query)
}



/// Trait for controlling media playback (Commands)
pub trait MediaController: Send + Sync {
    fn try_play_pause(&self);
    fn try_next(&self);
    fn try_prev(&self);
}

/// Trait for monitoring media state (Events)
pub trait MediaMonitor {
    /// Starts the background listener thread
    /// Updates are sent via the provided channel.
    fn start(&self, tx: Sender<MediaTrackInfo>);
}

// ==============================================================
// OS SELECTION FACTORY
// ==============================================================

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub type PlatformMedia = windows::WindowsMediaManager;


#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub type PlatformMedia = linux::LinuxMediaManager;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub type PlatformMedia = macos::MacMediaManager;

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod dummy;
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub type PlatformMedia = dummy::DummyMediaManager;



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_encode_security() {
        // 1. Injection Attempt: Trying to add a fake parameter
        // If not encoded, this would be interpreted as two parameters: "search=Song" and "admin=true"
        let malicious = "Song&admin=true";
        assert_eq!(url_encode(malicious), "Song%26admin%3Dtrue");

        // 2. Path Traversal / Special Chars
        // '/' and '?' are reserved in URLs and must be encoded
        let messy = "AC/DC - Who Made Who?";
        // Expect: AC%2FDC+-+Who+Made+Who%3F
        // / -> %2F, space -> +, ? -> %3F
        assert_eq!(url_encode(messy), "AC%2FDC+-+Who+Made+Who%3F");
    }
}
