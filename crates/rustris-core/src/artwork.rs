//! D-011: automatic cover and banner artwork.
//!
//! The provider, matching and selection rules are adapted from Charles's
//! LutrisArtFetcher (MIT, revision f271b86): SteamGridDB first when the user has
//! entered a key, then the Steam Store. Rustris owns the downloaded files under
//! its data directory; nothing here touches a user-chosen local artwork file.
//!
//! Runs only on the artwork worker thread. Every request is HTTPS to a fixed host
//! list with no redirects, bounded in time and size; images are checked by their
//! signature before they are written atomically as private files.
use crate::{Error, Result, error::io, paths::Paths};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use uuid::Uuid;

const SETTINGS: &str = "artwork.toml";
const SGDB: &str = "https://www.steamgriddb.com/api/v2";
const STORE: &str = "https://store.steampowered.com/api";
const STEAM_CDN: &str = "https://shared.steamstatic.com/store_item_assets/steam/apps";
const MAX_JSON: u64 = 1024 * 1024;
const MAX_IMAGE: u64 = 16 * 1024 * 1024;
const MAX_KEY: usize = 256;

/// Persisted in a private `artwork.toml`; the key never reaches the interface.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Off until the user turns it on (D-011): game names leave this computer.
    pub enabled: bool,
    pub steamgriddb_key: Option<String>,
}
/// What the interface may show about the settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub enabled: bool,
    pub has_key: bool,
}
/// A requested settings change; `None` keeps the stored key.
#[derive(Clone, Debug)]
pub struct Change {
    pub enabled: bool,
    pub key: Option<KeyChange>,
}
#[derive(Clone, Debug)]
pub enum KeyChange {
    Set(String),
    Clear,
}

pub fn read_settings(paths: &Paths) -> Result<Settings> {
    let Some(text) = crate::private_config::read(&paths.config, SETTINGS, 64 * 1024)? else {
        return Ok(Settings::default());
    };
    toml::from_str(&text).map_err(|_| {
        Error::Invalid(
            "Artwork settings could not be read. Repair or back up artwork.toml before changing them."
                .into(),
        )
    })
}
pub fn status(paths: &Paths) -> Result<Status> {
    let settings = read_settings(paths)?;
    Ok(Status {
        enabled: settings.enabled,
        has_key: settings.steamgriddb_key.is_some(),
    })
}
pub fn change_settings(paths: &Paths, change: Change) -> Result<()> {
    let mut settings = read_settings(paths)?;
    settings.enabled = change.enabled;
    match change.key {
        Some(KeyChange::Set(key)) => {
            let key = key.trim().to_owned();
            if key.is_empty()
                || key.len() > MAX_KEY
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                return Err(Error::Invalid(
                    "A SteamGridDB API key contains only letters and digits.".into(),
                ));
            }
            settings.steamgriddb_key = Some(key);
        }
        Some(KeyChange::Clear) => settings.steamgriddb_key = None,
        None => {}
    }
    let text = toml::to_string(&settings).map_err(|e| Error::Invalid(e.to_string()))?;
    crate::private_config::replace(&paths.config, SETTINGS, &text)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Portrait 2:3 library cover.
    Cover,
    /// Wide banner behind a game's page.
    Hero,
}
impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Hero => "hero",
        }
    }
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "cover" => Ok(Self::Cover),
            "hero" => Ok(Self::Hero),
            _ => Err(Error::Database("Unknown fetched artwork kind".into())),
        }
    }
}
/// A file Rustris downloaded and now owns.
#[derive(Clone, Debug)]
pub struct Fetched {
    pub kind: Kind,
    pub path: PathBuf,
    pub source: &'static str,
}
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub files: Vec<Fetched>,
    /// Shown to the user when nothing usable was found.
    pub note: Option<String>,
}

/// Where a game's downloaded artwork lives; removed with the game.
pub fn directory(paths: &Paths, game: Uuid) -> PathBuf {
    paths.data.join("artwork").join(game.to_string())
}

/// Find and download artwork for one game. Network and file work only; the
/// caller records the result through the storage worker.
pub fn fetch(
    paths: &Paths,
    game: Uuid,
    title: &str,
    want: &[Kind],
    cancel: &AtomicBool,
) -> Result<Outcome> {
    let settings = read_settings(paths)?;
    if !settings.enabled {
        return Err(Error::Invalid(
            "Automatic artwork is off. Turn it on in Settings first.".into(),
        ));
    }
    let http = Http::new()?;
    let mut outcome = Outcome::default();
    let mut missing: Vec<Kind> = want.to_vec();
    let mut notes = Vec::new();

    if let Some(key) = settings.steamgriddb_key.as_deref() {
        match steamgriddb(&http, key, title, &missing, cancel) {
            Ok(found) => {
                for (kind, bytes) in found {
                    match save(paths, game, kind, &bytes, "steamgriddb") {
                        Ok(file) => {
                            outcome.files.push(file);
                            missing.retain(|k| *k != kind);
                        }
                        Err(error) => notes.push(error.to_string()),
                    }
                }
            }
            Err(error) => notes.push(format!("SteamGridDB: {error}")),
        }
    }
    if !missing.is_empty() && !cancel.load(Ordering::Acquire) {
        match steam_store(&http, title, &missing, cancel) {
            Ok(found) => {
                for (kind, bytes) in found {
                    match save(paths, game, kind, &bytes, "steam") {
                        Ok(file) => {
                            outcome.files.push(file);
                            missing.retain(|k| *k != kind);
                        }
                        Err(error) => notes.push(error.to_string()),
                    }
                }
            }
            Err(error) => notes.push(format!("Steam Store: {error}")),
        }
    }
    if outcome.files.is_empty() {
        let mut note = if notes.is_empty() {
            format!("No confident artwork match was found for “{title}”.")
        } else {
            format!("No artwork found for “{title}”. {}.", notes.join("; "))
        };
        if settings.steamgriddb_key.is_none() {
            note.push_str(" Games that are not on Steam need a SteamGridDB key in Settings.");
        }
        outcome.note = Some(note);
    }
    Ok(outcome)
}

struct Http {
    agent: ureq::Agent,
}
impl Http {
    fn new() -> Result<Self> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent("Rustris artwork")
            .build()
            .into();
        Ok(Self { agent })
    }
    fn json<T: for<'de> Deserialize<'de>>(&self, url: &str, bearer: Option<&str>) -> Result<T> {
        allowed(url)?;
        let mut request = self.agent.get(url);
        if let Some(key) = bearer {
            request = request.header("Authorization", format!("Bearer {key}"));
        }
        let mut response = request.call().map_err(network)?;
        let text = response
            .body_mut()
            .with_config()
            .limit(MAX_JSON)
            .read_to_string()
            .map_err(network)?;
        serde_json::from_str(&text)
            .map_err(|_| Error::Invalid("the service returned an unexpected response".into()))
    }
    fn image(&self, url: &str) -> Result<Vec<u8>> {
        allowed(url)?;
        let mut response = self.agent.get(url).call().map_err(network)?;
        let bytes = response
            .body_mut()
            .with_config()
            .limit(MAX_IMAGE)
            .read_to_vec()
            .map_err(network)?;
        extension(&bytes).ok_or_else(|| {
            Error::Invalid("the downloaded file is not a PNG, JPEG or WebP image".into())
        })?;
        Ok(bytes)
    }
}
fn network(error: ureq::Error) -> Error {
    match error {
        ureq::Error::StatusCode(401 | 403) => {
            Error::Invalid("the service refused the request (check the API key)".into())
        }
        ureq::Error::StatusCode(404) => Error::Invalid("not found".into()),
        ureq::Error::StatusCode(code) => Error::Invalid(format!("the service answered {code}")),
        ureq::Error::Timeout(_) => Error::Invalid("the request timed out".into()),
        ureq::Error::BodyExceedsLimit(_) => Error::Invalid("the response was too large".into()),
        other => Error::Invalid(format!("the request failed ({other})")),
    }
}
/// HTTPS to the two services and their image CDNs only.
fn allowed(url: &str) -> Result<()> {
    let host = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .unwrap_or_default();
    let ok = !host.contains(['@', ':'])
        && (host == "www.steamgriddb.com"
            || host.ends_with(".steamgriddb.com")
            || host == "store.steampowered.com"
            || host.ends_with(".steamstatic.com")
            || host.ends_with(".akamaihd.net"));
    if ok {
        Ok(())
    } else {
        Err(Error::Invalid(
            "an artwork address outside the allowed services was refused".into(),
        ))
    }
}
fn extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}
fn encode(segment: &str) -> String {
    let mut out = String::new();
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// --- SteamGridDB ----------------------------------------------------------

#[derive(Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct Envelope<T> {
    #[serde(default = "Vec::new")]
    data: Vec<T>,
}
#[derive(Deserialize)]
struct SearchResult {
    id: u64,
    name: String,
}
#[derive(Deserialize)]
struct Asset {
    url: String,
    #[serde(default)]
    nsfw: bool,
    #[serde(default)]
    humor: bool,
}

fn steamgriddb(
    http: &Http,
    key: &str,
    title: &str,
    want: &[Kind],
    cancel: &AtomicBool,
) -> Result<Vec<(Kind, Vec<u8>)>> {
    let results: Envelope<SearchResult> = http.json(
        &format!("{SGDB}/search/autocomplete/{}", encode(title.trim())),
        Some(key),
    )?;
    let target = normalize_title(title);
    // LutrisArtFetcher takes the first hit; Rustris also requires a confident name match.
    let game = results
        .data
        .iter()
        .take(10)
        .map(|r| (match_score(&target, &normalize_title(&r.name)), r))
        .filter(|(score, _)| *score >= 75)
        // Highest score; among equals, the closest (shortest) name, then the first hit.
        .min_by_key(|(score, r)| (std::cmp::Reverse(*score), r.name.len()))
        .map(|(_, r)| r.id)
        .ok_or_else(|| Error::Invalid("no confident match".into()))?;
    let mut found = Vec::new();
    for kind in want {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        let path = match kind {
            Kind::Cover => {
                format!("{SGDB}/grids/game/{game}?dimensions=600x900&nsfw=false&humor=false")
            }
            Kind::Hero => format!("{SGDB}/heroes/game/{game}?nsfw=false&humor=false"),
        };
        let assets: Envelope<Asset> = http.json(&path, Some(key))?;
        if let Some(asset) = assets.data.iter().find(|a| !a.nsfw && !a.humor) {
            found.push((*kind, http.image(&asset.url)?));
        }
    }
    Ok(found)
}

// --- Steam Store fallback ---------------------------------------------------

#[derive(Deserialize)]
struct StoreSearch {
    #[serde(default)]
    items: Vec<StoreItem>,
}
#[derive(Deserialize)]
struct StoreItem {
    id: u32,
    name: String,
    #[serde(default, rename = "type")]
    kind: String,
}

fn steam_store(
    http: &Http,
    title: &str,
    want: &[Kind],
    cancel: &AtomicBool,
) -> Result<Vec<(Kind, Vec<u8>)>> {
    let search: StoreSearch = http.json(
        &format!(
            "{STORE}/storesearch/?term={}&l=english&cc=us",
            encode(title.trim())
        ),
        None,
    )?;
    let target = normalize_title(title);
    let mut scored: Vec<(i32, &StoreItem)> = Vec::new();
    let mut seen = HashSet::new();
    for item in search.items.iter().take(15) {
        if is_non_game_type(&item.kind) || !seen.insert(item.id) {
            continue;
        }
        let score = match_score(&target, &normalize_title(&item.name));
        if score >= 60 {
            scored.push((score, item));
        }
    }
    scored.sort_by_key(|(score, item)| (std::cmp::Reverse(*score), item.name.len()));
    let (best, best_item) = *scored
        .first()
        .ok_or_else(|| Error::Invalid("no confident match".into()))?;
    let second = scored.get(1).map_or(0, |(score, _)| *score);
    let contains_target = !target.is_empty()
        && format!(" {} ", normalize_title(&best_item.name)).contains(&format!(" {target} "));
    // LutrisArtFetcher's confidence rule.
    let confident = best >= 85
        || (best >= 75 && (second == 0 || best - second >= 10))
        || (best >= 80 && contains_target);
    if !confident {
        return Err(Error::Invalid("no confident match".into()));
    }
    let app = best_item.id;
    let mut found = Vec::new();
    for kind in want {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        let candidates = match kind {
            Kind::Cover => vec![
                format!("{STEAM_CDN}/{app}/library_600x900_2x.jpg"),
                format!("{STEAM_CDN}/{app}/library_600x900.jpg"),
            ],
            Kind::Hero => vec![
                format!("{STEAM_CDN}/{app}/library_hero.jpg"),
                format!("{STEAM_CDN}/{app}/header.jpg"),
            ],
        };
        for url in candidates {
            if let Ok(bytes) = http.image(&url) {
                found.push((*kind, bytes));
                break;
            }
        }
    }
    Ok(found)
}

// --- Matching (adapted from LutrisArtFetcher) -------------------------------

pub(crate) fn normalize_title(input: &str) -> String {
    input
        .chars()
        .flat_map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().collect::<Vec<_>>()
            } else {
                vec![' ']
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn token_overlap_score(target: &str, candidate: &str) -> i32 {
    let target: HashSet<&str> = target.split_whitespace().collect();
    let candidate: HashSet<&str> = candidate.split_whitespace().collect();
    if target.is_empty() || candidate.is_empty() {
        return 0;
    }
    let common = target.intersection(&candidate).count();
    let union = target.union(&candidate).count().max(1);
    i32::try_from(common.saturating_mul(100) / union).unwrap_or(i32::MAX)
}
pub(crate) fn match_score(target: &str, candidate: &str) -> i32 {
    if target.is_empty() || candidate.is_empty() {
        return 0;
    }
    if target == candidate {
        return 100;
    }
    // Whole words only: "It" must not match "Split Fiction".
    let (target_words, candidate_words) = (format!(" {target} "), format!(" {candidate} "));
    if target_words.contains(&candidate_words) || candidate_words.contains(&target_words) {
        return 82;
    }
    let mut score = token_overlap_score(target, candidate);
    if target_words.starts_with(candidate_words.trim_end())
        || candidate_words.starts_with(target_words.trim_end())
    {
        score += 8;
    }
    score.min(95)
}
fn is_non_game_type(kind: &str) -> bool {
    matches!(
        kind.to_ascii_lowercase().as_str(),
        "dlc" | "demo" | "advertising" | "video" | "movie" | "episode" | "series" | "mod" | "music"
    )
}

// --- Owned files ------------------------------------------------------------

fn save(
    paths: &Paths,
    game: Uuid,
    kind: Kind,
    bytes: &[u8],
    source: &'static str,
) -> Result<Fetched> {
    let extension = extension(bytes)
        .ok_or_else(|| Error::Invalid("The downloaded file is not an image.".into()))?;
    let directory = directory(paths, game);
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .map_err(|e| io("create the artwork folder", e))?;
    let name = format!("{}-{}.{extension}", kind.key(), Uuid::now_v7());
    let partial = directory.join(format!(".{name}.partial"));
    let path = directory.join(&name);
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&partial)
            .map_err(|e| io("create a downloaded artwork file", e))?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| io("write a downloaded artwork file", e))?;
        fs::rename(&partial, &path).map_err(|e| io("publish a downloaded artwork file", e))?;
        fs::File::open(&directory)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| io("sync the artwork folder", e))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result?;
    Ok(Fetched { kind, path, source })
}

/// Remove a file Rustris downloaded, only if it is inside its own artwork folder.
pub fn remove_owned(paths: &Paths, path: &Path) {
    let root = paths.data.join("artwork");
    if path.starts_with(&root)
        && !path
            .components()
            .any(|c| c == std::path::Component::ParentDir)
        && let Err(error) = fs::remove_file(path)
    {
        tracing::debug!("Could not remove replaced artwork: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_follows_lutris_art_fetcher_rules() {
        let target = normalize_title("SKATE 3");
        assert_eq!(target, "skate 3");
        assert_eq!(match_score(&target, &normalize_title("Skate 3")), 100);
        assert_eq!(match_score(&target, &normalize_title("skate 3 deluxe")), 82);
        assert!(match_score(&target, &normalize_title("Session: Skate Sim")) < 60);
        assert!(is_non_game_type("DLC"));
        // Containment counts whole words only, and case folding covers non-ASCII.
        assert!(match_score("it", &normalize_title("Split Fiction")) < 60);
        assert!(match_score("ori", &normalize_title("Glorious Companions")) < 60);
        assert_eq!(
            match_score(&normalize_title("ÖKOSYSTEM"), &normalize_title("Ökosystem")),
            100
        );
    }

    #[test]
    fn only_https_to_the_artwork_services_is_allowed() {
        assert!(allowed("https://cdn2.steamgriddb.com/grid/x.png").is_ok());
        assert!(allowed("https://shared.steamstatic.com/a/b.jpg").is_ok());
        assert!(allowed("https://store.steampowered.com/api/storesearch/").is_ok());
        assert!(allowed("http://cdn2.steamgriddb.com/grid/x.png").is_err());
        assert!(allowed("https://evil.example/steamgriddb.com").is_err());
        assert!(allowed("https://user@cdn2.steamgriddb.com/x").is_err());
        assert!(allowed("https://steamgriddb.com.evil.example/x").is_err());
    }

    #[test]
    fn images_are_recognised_by_signature_and_paths_are_encoded() {
        assert_eq!(extension(b"\x89PNG\r\n\x1a\nrest"), Some("png"));
        assert_eq!(extension(&[0xff, 0xd8, 0xff, 0xe0]), Some("jpg"));
        assert_eq!(extension(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(extension(b"<html>"), None);
        assert_eq!(
            encode("Half-Life 2: Episode/One"),
            "Half-Life%202%3A%20Episode%2FOne"
        );
    }
}

// --- Background fetch worker ------------------------------------------------

/// Delivers each game's result as soon as it is known.
pub type Deliver = Box<dyn FnMut(Uuid, Result<Outcome>) + Send>;
struct Job {
    games: Vec<(Uuid, String)>,
    want: Vec<Kind>,
    deliver: Deliver,
}

/// One serialized network worker, separate from storage and sessions so slow
/// services never delay library or launch work. The queue is bounded.
pub struct Fetcher {
    sender: Option<std::sync::mpsc::SyncSender<Option<Job>>>,
    cancel: std::sync::Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}
#[derive(Clone)]
pub struct FetchClient {
    sender: std::sync::mpsc::SyncSender<Option<Job>>,
}
impl FetchClient {
    /// Queue games without blocking; a full queue is reported, not waited on.
    pub fn submit(
        &self,
        games: Vec<(Uuid, String)>,
        want: Vec<Kind>,
        deliver: Deliver,
    ) -> Result<()> {
        self.sender
            .try_send(Some(Job {
                games,
                want,
                deliver,
            }))
            .map_err(|error| match error {
                std::sync::mpsc::TrySendError::Full(_) => Error::Invalid(
                    "Artwork downloads are already queued. Try again when they finish.".into(),
                ),
                std::sync::mpsc::TrySendError::Disconnected(_) => Error::WorkerClosed,
            })
    }
}
impl Fetcher {
    pub fn start(paths: Paths) -> Result<Self> {
        let (sender, receiver) = std::sync::mpsc::sync_channel::<Option<Job>>(16);
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let join = std::thread::Builder::new()
            .name("rustris-artwork".into())
            .spawn(move || {
                // `None` is the stop message: clients can outlive the worker handle.
                while let Ok(Some(mut job)) = receiver.recv() {
                    for (game, title) in job.games {
                        if stop.load(Ordering::Acquire) {
                            return;
                        }
                        let result = fetch(&paths, game, &title, &job.want, &stop);
                        (job.deliver)(game, result);
                    }
                }
            })
            .map_err(|e| io("start the artwork worker", e))?;
        Ok(Self {
            sender: Some(sender),
            cancel,
            join: Some(join),
        })
    }
    pub fn client(&self) -> Option<FetchClient> {
        self.sender.clone().map(|sender| FetchClient { sender })
    }
    /// Stop between requests; an in-flight request ends within its timeout.
    pub fn shutdown(mut self) -> Result<()> {
        self.cancel.store(true, Ordering::Release);
        if let Some(sender) = self.sender.take() {
            // Queued jobs are skipped once `cancel` is set; `None` ends the loop.
            let _ = sender.send(None);
        }
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| Error::Invalid("The artwork worker stopped unexpectedly.".into()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod worker_tests {
    use super::*;

    #[test]
    fn shutdown_returns_while_a_client_is_still_held() {
        let root = std::env::temp_dir().join(format!("rustris-artwork-{}", Uuid::now_v7()));
        let paths = Paths::in_root(&root);
        let fetcher = Fetcher::start(paths).unwrap_or_else(|e| panic!("{e}"));
        let _client = fetcher.client();
        let started = std::time::Instant::now();
        fetcher.shutdown().unwrap_or_else(|e| panic!("{e}"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
