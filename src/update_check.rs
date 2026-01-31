use serde::Deserialize;
use std::error::Error;
use semver::Version;

#[derive(Deserialize, Debug)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub html_url: String, // Link to the release page
}

pub fn check_for_updates() -> Result<Option<String>, Box<dyn Error>> {
    let current_version_str = env!("CARGO_PKG_VERSION");
    
    let local_version = Version::parse(current_version_str)
        .map_err(|e| format!("Critical: Local version '{}' is not SemVer compliant: {}", current_version_str, e))?;
    
    // User-Agent is REQUIRED by GitHub API
    let resp = ureq::get("https://api.github.com/repos/BeSpec-Dev/bespec/releases/latest")
        .set("User-Agent", "bespec-client")
        .call()?;

    let release: GitHubRelease = resp.into_json()?;

    // handle 'v' prefix (v1.5.1 vs 1.5.1)
    let clean_tag = release.tag_name.trim_start_matches('v');

    // Parse local and remote versions
    match Version::parse(clean_tag) {
        Ok(remote_version) => {
            //tracing::info!("[Update] Local: {}, Remote: {}", env!("CARGO_PKG_VERSION"), remote_version);
            // Only notify if remote is strictly greater than local
            if remote_version > local_version {
                Ok(Some(release.html_url))
            } else {
                Ok(None)
            }
        }
        Err(e) => {
            // Log warning but don't crash. Return Ok(None) to ignore this update
            tracing::warn!("[Update] Ignoring non-SemVer release tag '{}': {}",release.tag_name, e );
            Ok(None)
        }
    }
}