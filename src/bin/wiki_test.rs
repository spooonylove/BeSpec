use serde_json:: Value;

// 1. SIMULATED STRUCT
struct Track {
    artist: String,
    title: String,
    album: String,
}


fn sanitize_title(title: &str) -> String {
    // A. Sanatize: Remove common "garbage" from titles that confuseds search
    // e.g. "Song Name (Offiocial Video)" -> "Song Name"
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

    let mut clean_title = title.to_string();
    
    for term in garbage_terms.iter() {
        if let Some(idx) = clean_title.to_lowercase().find(&term.to_lowercase()) {
            clean_title.truncate(idx);
        }
    }
    clean_title.trim().to_string()
}

fn fetch_wikipedia_url(track: &Track) -> String {
    // A. Clean the inputs
    let clean_title = sanitize_title(&track.title);
    
    // B. Construct the Search Query
    // Full text search handles "Artist Album" much better than prefix search.
    let search_query = if !track.album.is_empty() && track.album != "Unknown Album" {
        format!("{} {}", track.artist, track.album)
    } else {
        format!("{} {}", track.artist, clean_title)
    };

    println!("   > Querying API for: '{}'...", search_query);

    // C. Call MediaWiki API (Full Text Search)
    // We switched from 'opensearch' (autocomplete) to 'list=search' (relevance).
    let api_url = "https://en.wikipedia.org/w/api.php";
    
    let resp = ureq::get(api_url)
        .query("action", "query")
        .query("list", "search")
        .query("srsearch", &search_query)
        .query("srlimit", "1")
        .query("format", "json")
        .call();

    match resp {
        Ok(response) => {
            if let Ok(json) = serde_json::from_reader::<_, Value>(response.into_reader()) {
                
                // --- DEBUG BLOCK ---
                // println!("     [DEBUG] Raw JSON: {}", json);
                // -------------------

                // Parse Path: query -> search -> [0] -> title
                if let Some(title) = json.get("query")
                    .and_then(|q| q.get("search"))
                    .and_then(|s| s.get(0)) // Get first result
                    .and_then(|r| r.get("title"))
                    .and_then(|t| t.as_str()) 
                {
                    // Success! Construct the specific article URL.
                    // Wikipedia URLs use underscores instead of spaces.
                    let url_slug = title.replace(" ", "_");
                    // We must also URL-encode special chars (like & -> %26) just in case,
                    // though simple replacement usually works for Wiki slugs.
                    return format!("https://en.wikipedia.org/wiki/{}", url_slug);
                } else {
                     println!("     [DEBUG] No results found in 'query.search'.");
                }
            } else {
                println!("     [DEBUG] Failed to parse JSON response.");
            }
        },
        Err(e) => {
            println!("     [DEBUG] HTTP Request Failed: {}", e);
        }
    }

    // Fallback if API fails or finds nothing:
    // This "Special:Search" link will perform the search in the user's browser.
    format!("https://en.wikipedia.org/w/index.php?search={}", search_query.replace(" ", "+"))
    
    
}

fn main() {
    // 3. THE TEST CASES
    let tests = vec![
        // 1. Standard Pop (Easy)
        Track {
            artist: "Pink Floyd".into(),
            title: "Time".into(),
            album: "The Dark Side of the Moon".into(),
        },
        // 2. Modern Pop (Easy)
        Track {
            artist: "Dua Lipa".into(),
            title: "Levitating".into(),
            album: "Future Nostalgia".into(),
        },
        // 3. Messy YouTube Title (Needs Sanitization)
        Track {
            artist: "Rick Astley".into(),
            title: "Never Gonna Give You Up (Official Video)".into(),
            album: "".into(), 
        },
        // 4. Band with Symbols
        Track {
            artist: "AC/DC".into(),
            title: "Back In Black".into(),
            album: "Back In Black".into(),
        },
        // 5. Single / No Album (Search fallback to title)
        Track {
            artist: "Childish Gambino".into(),
            title: "This Is America".into(),
            album: "".into(),
        },
        // 6. "Feat." in Title (Sanitization check)
        Track {
            artist: "Daft Punk".into(),
            title: "Get Lucky (feat. Pharrell Williams)".into(),
            album: "Random Access Memories".into(),
        },
        // 7. "Feat." in Artist String
        Track {
            artist: "Calvin Harris feat. Rihanna".into(),
            title: "This Is What You Came For".into(),
            album: "".into(),
        },
        // 8. Remixes (Often have weird titles)
        Track {
            artist: "Lana Del Rey".into(),
            title: "Summertime Sadness (Cedric Gervais Remix)".into(),
            album: "".into(),
        },
        // 9. Classical Music (Composer vs Artist ambiguity)
        Track {
            artist: "Ludwig van Beethoven".into(),
            title: "Symphony No. 9".into(),
            album: "".into(),
        },
        // 10. Ambiguous Band Names (e.g., "Yes", "Live")
        Track {
            artist: "Yes".into(),
            title: "Owner of a Lonely Heart".into(),
            album: "90125".into(),
        },
        // 11. Soundtrack / Film Score
        Track {
            artist: "Hans Zimmer".into(),
            title: "Time".into(),
            album: "Inception".into(),
        },
        // 12. Unicode / Foreign Language
        Track {
            artist: "Hikaru Utada".into(),
            title: "First Love".into(),
            album: "First Love".into(),
        },
    ];

    println!("--- Testing Wikipedia Link Logic ---\n");

    for t in tests {
        println!("Input: {} - {} (Album: {})", t.artist, t.title, t.album);
        let url  = fetch_wikipedia_url(&t);
        println!("Generated Wikipedia URL: {}\n", url);
    }
}