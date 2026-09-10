use std::time::Duration;

use semver::Version;
use serde::Deserialize;

const LATEST_URL: &str = "https://api.github.com/repos/Mo999salah/wali/releases/latest";
const RELEASE_URL_PREFIX: &str = "https://github.com/Mo999salah/wali/releases/";
const USER_AGENT: &str = concat!("Wali/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    UpToDate,
    Available { version: String, html_url: String },
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
}

pub fn parse_release_version(tag: &str) -> Result<Version, String> {
    let trimmed = tag.trim();
    if trimmed.is_empty() {
        return Err("empty version".into());
    }
    let rest = trimmed.strip_prefix('v').unwrap_or(trimmed);
    Version::parse(rest).map_err(|e| format!("invalid version {trimmed}: {e}"))
}

pub fn is_release_url(url: &str) -> bool {
    url.starts_with(RELEASE_URL_PREFIX)
        && url.bytes().all(|b| {
            b.is_ascii() && !b.is_ascii_control() && !b.is_ascii_whitespace() && b != b'\\'
        })
}

fn classify(current: &Version, remote: GithubRelease) -> Result<Outcome, String> {
    if remote.prerelease || remote.draft {
        return Ok(Outcome::UpToDate);
    }
    let version = parse_release_version(&remote.tag_name)?;
    if !version.pre.is_empty() {
        return Ok(Outcome::UpToDate);
    }
    if version > *current {
        if !is_release_url(&remote.html_url) {
            return Err("unexpected release url".into());
        }
        Ok(Outcome::Available {
            version: version.to_string(),
            html_url: remote.html_url,
        })
    } else {
        Ok(Outcome::UpToDate)
    }
}

pub fn check_latest(current: &str) -> Result<Outcome, String> {
    let current = parse_release_version(current)?;
    match fetch_latest() {
        Ok(None) => Ok(Outcome::UpToDate),
        Ok(Some(remote)) => classify(&current, remote),
        Err(e) => Err(e),
    }
}

fn fetch_latest() -> Result<Option<GithubRelease>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .build();
    match agent
        .get(LATEST_URL)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(resp) => {
            let body = resp.into_string().map_err(|e| format!("body: {e}"))?;
            serde_json::from_str(&body)
                .map(Some)
                .map_err(|e| format!("decode: {e}"))
        }
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(e) => Err(format!("request: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, url: &str) -> GithubRelease {
        GithubRelease {
            tag_name: tag.into(),
            html_url: url.into(),
            prerelease: false,
            draft: false,
        }
    }

    #[test]
    fn strips_v_prefix() {
        assert_eq!(
            parse_release_version("v0.2.0").unwrap(),
            Version::new(0, 2, 0)
        );
        assert_eq!(
            parse_release_version("0.2.0").unwrap(),
            Version::new(0, 2, 0)
        );
    }

    #[test]
    fn rejects_invalid_versions() {
        assert!(parse_release_version("").is_err());
        assert!(parse_release_version("v").is_err());
        assert!(parse_release_version("latest").is_err());
        assert!(parse_release_version("v1").is_err());
        assert!(parse_release_version("0.1").is_err());
    }

    #[test]
    fn compares_semver_not_lexicographic() {
        let current = Version::new(0, 1, 0);
        let gh = |tag: &str| format!("https://github.com/Mo999salah/wali/releases/tag/{tag}");
        let available = |tag: &str, ver: &str| {
            classify(&current, release(tag, &gh(tag))).unwrap()
                == Outcome::Available {
                    version: ver.into(),
                    html_url: gh(tag),
                }
        };

        assert!(available("v0.1.1", "0.1.1"));
        assert!(available("v0.2.0", "0.2.0"));
        assert!(available("v1.0.0", "1.0.0"));

        let older_current = Version::parse("0.1.9").unwrap();
        assert_eq!(
            classify(&older_current, release("v0.2.0", &gh("v0.2.0"))).unwrap(),
            Outcome::Available {
                version: "0.2.0".into(),
                html_url: gh("v0.2.0"),
            }
        );
        let almost = Version::parse("0.99.0").unwrap();
        assert_eq!(
            classify(&almost, release("v1.0.0", &gh("v1.0.0"))).unwrap(),
            Outcome::Available {
                version: "1.0.0".into(),
                html_url: gh("v1.0.0"),
            }
        );
    }

    #[test]
    fn same_or_older_is_up_to_date() {
        let current = Version::new(0, 2, 0);
        let gh = "https://github.com/Mo999salah/wali/releases/tag/v0.2.0";
        assert_eq!(
            classify(&current, release("v0.2.0", gh)).unwrap(),
            Outcome::UpToDate
        );
        assert_eq!(
            classify(&current, release("v0.1.9", gh)).unwrap(),
            Outcome::UpToDate
        );
    }

    #[test]
    fn skips_prerelease_and_draft() {
        let current = Version::new(0, 1, 0);
        let url = "https://github.com/Mo999salah/wali/releases/tag/v0.2.0";
        let mut pre = release("v0.2.0", url);
        pre.prerelease = true;
        assert_eq!(classify(&current, pre).unwrap(), Outcome::UpToDate);

        let mut draft = release("v0.2.0", url);
        draft.draft = true;
        assert_eq!(classify(&current, draft).unwrap(), Outcome::UpToDate);

        assert_eq!(
            classify(&current, release("v0.2.0-rc.1", url)).unwrap(),
            Outcome::UpToDate
        );
    }

    #[test]
    fn rejects_non_release_url() {
        let current = Version::new(0, 1, 0);
        assert!(classify(
            &current,
            release("v0.2.0", "https://evil.example/releases/tag/v0.2.0")
        )
        .is_err());
        assert!(!is_release_url(
            "https://github.com/Mo999salah/wali/releases/tag/v0.2.0\n"
        ));
        assert!(!is_release_url(
            "https://github.com/other/wali/releases/tag/v0.2.0"
        ));
        assert!(is_release_url(
            "https://github.com/Mo999salah/wali/releases/tag/v0.2.0"
        ));
    }
}
