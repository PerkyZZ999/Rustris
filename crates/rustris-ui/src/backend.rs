#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        #[qobject]
        type QAbstractListModel;
    }
    unsafe extern "C++" {
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[base=QAbstractListModel]
        #[qml_element]
        #[qproperty(bool, ready)]
        #[qproperty(bool, busy)]
        #[qproperty(bool, closing)]
        #[qproperty(bool, close_ready)]
        #[qproperty(bool, opaque)]
        #[qproperty(bool, reduced_motion)]
        #[qproperty(i32, total)]
        #[qproperty(i32, shown)]
        #[qproperty(i32, favorite_total)]
        #[qproperty(i32, selected_row)]
        #[qproperty(QString, selected_id)]
        #[qproperty(QString, detail_json)]
        #[qproperty(QString, draft_json)]
        #[qproperty(QString, error)]
        #[qproperty(QString, form_error)]
        #[qproperty(bool, artwork_enabled)]
        #[qproperty(bool, artwork_has_key)]
        #[qproperty(bool, artwork_fetching)]
        #[qproperty(QString, artwork_status)]
        type Library = super::LibraryRust;
        #[qinvokable]
        fn load(self: Pin<&mut Library>);
        #[qinvokable]
        fn filter(self: Pin<&mut Library>, query: &QString, favorites: bool);
        #[qinvokable]
        fn select(self: Pin<&mut Library>, id: &QString);
        #[qinvokable]
        fn step(self: Pin<&mut Library>, direction: i32);
        #[qinvokable]
        fn favorite(self: Pin<&mut Library>, id: &QString, value: bool);
        #[qinvokable]
        fn set_artwork(self: Pin<&mut Library>, id: &QString, url: &QUrl);
        #[qinvokable]
        fn set_artwork_path(self: Pin<&mut Library>, id: &QString, path: &QString, exact: bool);
        #[qinvokable]
        fn prepare_artwork_path(self: Pin<&mut Library>);
        #[qinvokable]
        fn clear_artwork(self: Pin<&mut Library>, id: &QString);
        #[qinvokable]
        fn fetch_artwork(self: Pin<&mut Library>, id: &QString);
        #[qinvokable]
        fn fetch_missing_artwork(self: Pin<&mut Library>);
        #[qinvokable]
        fn artwork_settings(self: Pin<&mut Library>, enabled: bool, key: &QString, clear_key: bool);
        #[qinvokable]
        fn create(self: Pin<&mut Library>);
        #[qinvokable]
        fn edit(self: Pin<&mut Library>, id: &QString);
        #[qinvokable]
        fn save(self: Pin<&mut Library>, json: &QString);
        #[qinvokable]
        fn convert(self: Pin<&mut Library>, json: &QString, exact: bool);
        #[qinvokable]
        fn remove(self: Pin<&mut Library>, id: &QString);
        #[qinvokable]
        fn preferences(self: Pin<&mut Library>, opaque: bool, reduced: bool);
        #[qinvokable]
        fn request_close(self: Pin<&mut Library>);
        #[qinvokable]
        fn cancel_close(self: Pin<&mut Library>);
        #[qsignal]
        fn open_editor(self: Pin<&mut Library>);
        #[qsignal]
        fn saved(self: Pin<&mut Library>);
        #[qsignal]
        fn artwork_saved(self: Pin<&mut Library>);
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &Library, parent: &QModelIndex) -> i32;
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &Library) -> QHash_i32_QByteArray;
        #[cxx_override]
        fn data(self: &Library, index: &QModelIndex, role: i32) -> QVariant;
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut Library>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut Library>);
        #[inherit]
        fn index(self: &Library, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(self: Pin<&mut Library>, top: &QModelIndex, bottom: &QModelIndex);
    }
    impl cxx_qt::Threading for Library {}
}

use crate::{CLIENT, FETCH_CLIENT, RUNTIME, draft::Draft};
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QUrl, QVariant,
};
use rustris_core::{
    Error, Result,
    bytes::OsBytes,
    domain::{self, Entry},
    paths::Settings,
    store::Snapshot,
    worker::{Command, Response},
};
use std::pin::Pin;
use uuid::Uuid;

/// D-011: the user's own artwork wins over a downloaded cover.
fn cover(entry: &Entry) -> Option<&OsBytes> {
    entry
        .artwork
        .as_ref()
        .filter(|path| !path.needs_byte_editor())
        .or(entry.fetched_cover.as_ref())
}

fn artwork_source(path: &OsBytes) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut source = String::from("image://rustris-art/");
    source.reserve(path.as_bytes().len() * 2);
    for byte in path.as_bytes() {
        source.push(char::from(HEX[usize::from(byte >> 4)]));
        source.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    source
}

#[derive(Default)]
pub struct LibraryRust {
    ready: bool,
    busy: bool,
    closing: bool,
    close_ready: bool,
    opaque: bool,
    reduced_motion: bool,
    total: i32,
    shown: i32,
    favorite_total: i32,
    selected_row: i32,
    selected_id: QString,
    detail_json: QString,
    draft_json: QString,
    error: QString,
    form_error: QString,
    artwork_enabled: bool,
    artwork_has_key: bool,
    artwork_fetching: bool,
    artwork_status: QString,
    /// Games queued on the artwork worker and not yet reported back.
    fetch_pending: usize,
    entries: Vec<Entry>,
    visible: Vec<usize>,
    query: String,
    favorites: bool,
    pending: usize,
    sequence: u64,
    last_snapshot: u64,
    last_selection: u64,
}

#[derive(Clone, Copy)]
enum Kind {
    Load,
    Save,
    Artwork,
    Selection,
    Other,
}

impl ffi::Library {
    fn dispatch(mut self: Pin<&mut Self>, command: Command, kind: Kind) -> bool {
        if *self.closing() {
            return false;
        }
        let Some(client) = CLIENT.get() else {
            self.set_error(QString::from("The library worker is unavailable."));
            return false;
        };
        let Some(runtime) = RUNTIME.get() else {
            self.set_error(QString::from(
                "The application worker runtime is unavailable.",
            ));
            return false;
        };
        let receiver = match client.submit(command) {
            Ok(receiver) => receiver,
            Err(error) => {
                self.show_error(error, kind);
                return false;
            }
        };
        let sequence = {
            let mut state = self.as_mut().rust_mut();
            state.sequence += 1;
            state.pending += 1;
            state.sequence
        };
        self.as_mut().set_busy(true);
        let handle = self.qt_thread();
        runtime.spawn(async move {
            let result = receiver.await.unwrap_or(Err(Error::WorkerClosed));
            if let Err(error) =
                handle.queue(move |backend| backend.complete(result, kind, sequence))
            {
                tracing::debug!(?error, "View closed before result delivery");
            }
        });
        true
    }

    fn complete(mut self: Pin<&mut Self>, result: Result<Response>, kind: Kind, sequence: u64) {
        let success = result.is_ok();
        match result {
            Ok(Response::Snapshot(snapshot)) => {
                if sequence >= self.last_snapshot {
                    self.as_mut().apply(snapshot, sequence);
                }
                if matches!(kind, Kind::Save) {
                    self.as_mut().set_form_error(QString::default());
                    self.as_mut().saved();
                    // D-011: a newly saved game without any cover gets one fetched.
                    let wanted = (*self.artwork_enabled())
                        .then(|| {
                            let selected = self.selected_id.to_string();
                            self.entries
                                .iter()
                                .find(|e| e.game_id.to_string() == selected)
                                .filter(|e| e.artwork.is_none() && e.fetched_cover.is_none())
                                .map(|e| (e.game_id, e.profile.name.clone()))
                        })
                        .flatten();
                    if let Some(game) = wanted {
                        self.as_mut().start_fetch(vec![game]);
                    }
                }
                if matches!(kind, Kind::Artwork) {
                    self.as_mut().set_form_error(QString::default());
                    self.as_mut().artwork_saved();
                }
            }
            Ok(Response::SelectionSaved) => {}
            Err(error) => {
                let committed = matches!(error, Error::CommittedRefresh(_));
                if matches!(kind, Kind::Load) || committed {
                    self.as_mut().set_ready(false);
                }
                self.as_mut().show_error(error, kind);
                if committed && matches!(kind, Kind::Save) {
                    self.as_mut().saved();
                }
                if committed && matches!(kind, Kind::Artwork) {
                    self.as_mut().artwork_saved();
                }
            }
        }
        let pending = {
            let mut state = self.as_mut().rust_mut();
            debug_assert!(state.pending > 0);
            state.pending -= 1;
            state.pending
        };
        self.as_mut().set_busy(pending > 0);
        if !success {
            self.as_mut().set_closing(false);
        }
        if pending == 0 && *self.closing() {
            self.set_close_ready(true);
        }
    }

    fn show_error(self: Pin<&mut Self>, error: Error, kind: Kind) {
        if matches!(kind, Kind::Save | Kind::Artwork)
            && !matches!(error, Error::CommittedRefresh(_))
        {
            self.set_form_error(QString::from(&error.to_string()));
        } else {
            self.set_error(QString::from(&error.to_string()));
        }
    }

    fn apply(mut self: Pin<&mut Self>, snapshot: Snapshot, sequence: u64) {
        let selected = if sequence < self.last_selection {
            Uuid::parse_str(&self.selected_id.to_string()).ok()
        } else {
            snapshot.selected
        };
        let warning = snapshot.warning.unwrap_or_default();
        self.as_mut().set_artwork_enabled(snapshot.artwork.enabled);
        self.as_mut().set_artwork_has_key(snapshot.artwork.has_key);
        let opaque = snapshot.settings.opaque;
        let reduced = snapshot.settings.reduced_motion;
        let visible = domain::filter(&snapshot.entries, &self.query, self.favorites);
        let same_order = self
            .visible
            .iter()
            .map(|&offset| self.entries[offset].game_id)
            .eq(visible
                .iter()
                .map(|&offset| snapshot.entries[offset].game_id));
        // No Rust field borrow survives Qt notifications, which may query roles.
        if !same_order {
            unsafe { self.as_mut().begin_reset_model() };
        }
        {
            let mut state = self.as_mut().rust_mut();
            state.entries = snapshot.entries;
            state.last_snapshot = sequence;
            state.visible = visible;
        }
        if !same_order {
            unsafe { self.as_mut().end_reset_model() };
        } else if !self.visible.is_empty() {
            let top = self.index(0, 0, &QModelIndex::default());
            let bottom = self.index(
                i32::try_from(self.visible.len() - 1).unwrap_or(i32::MAX),
                0,
                &QModelIndex::default(),
            );
            self.as_mut().data_changed(&top, &bottom);
        }
        let selected = selected.filter(|id| self.entries.iter().any(|entry| entry.game_id == *id));
        self.as_mut().set_selected_id(QString::from(
            &selected.map_or_else(String::new, |id| id.to_string()),
        ));
        self.as_mut().set_opaque(opaque);
        self.as_mut().set_reduced_motion(reduced);
        self.as_mut().set_error(QString::from(&warning));
        self.as_mut().set_ready(true);
        self.refresh_properties();
    }

    fn refresh_properties(mut self: Pin<&mut Self>) {
        let selected = self.selected_id.to_string();
        let row = self
            .visible
            .iter()
            .position(|&offset| self.entries[offset].game_id.to_string() == selected)
            .and_then(|n| i32::try_from(n).ok())
            .unwrap_or(-1);
        let detail=self.entries.iter().find(|entry|entry.game_id.to_string()==selected).map(|entry| {
            serde_json::json!({"id":entry.game_id.to_string(),"name":entry.profile.name,"favorite":entry.favorite,
                "executable":entry.profile.executable.label(),"cwd":entry.profile.cwd.label(),"runner":entry.profile.runner.key(),
                "prefix":entry.profile.prefix.as_ref().map(|p|p.label()),"availability":entry.availability,
                "review_approved":entry.review_approved,"argument_count":entry.profile.arguments.len(),"environment_count":entry.profile.environment.len(),"revision":entry.revision,
                "artwork_label":entry.artwork.as_ref().map(|p|p.label()),
                "artwork_url":cover(entry).map(artwork_source),
                "artwork_fetched":entry.artwork.is_none() && entry.fetched_cover.is_some(),
                "hero_url":entry.fetched_hero.as_ref().or(cover(entry)).map(artwork_source)})
        });
        let detail = detail.map_or_else(|| "null".into(), |value| value.to_string());
        let total = i32::try_from(self.entries.len()).unwrap_or(i32::MAX);
        let shown = i32::try_from(self.visible.len()).unwrap_or(i32::MAX);
        let favorite_total =
            i32::try_from(self.entries.iter().filter(|entry| entry.favorite).count())
                .unwrap_or(i32::MAX);
        self.as_mut().set_total(total);
        self.as_mut().set_shown(shown);
        self.as_mut().set_favorite_total(favorite_total);
        self.as_mut().set_selected_row(row);
        self.set_detail_json(QString::from(&detail));
    }

    pub fn load(self: Pin<&mut Self>) {
        self.dispatch(Command::Load, Kind::Load);
    }
    pub fn filter(mut self: Pin<&mut Self>, query: &QString, favorites: bool) {
        let visible = domain::filter(&self.entries, &query.to_string(), favorites);
        unsafe { self.as_mut().begin_reset_model() };
        {
            let mut state = self.as_mut().rust_mut();
            state.query = query.to_string();
            state.favorites = favorites;
            state.visible = visible;
        }
        unsafe { self.as_mut().end_reset_model() };
        self.refresh_properties();
    }
    pub fn select(mut self: Pin<&mut Self>, id: &QString) {
        let Ok(uuid) = Uuid::parse_str(&id.to_string()) else {
            return;
        };
        if !*self.ready()
            || !self.entries.iter().any(|entry| entry.game_id == uuid)
            || self.selected_id() == id
        {
            return;
        }
        if self
            .as_mut()
            .dispatch(Command::Select(uuid), Kind::Selection)
        {
            let sequence = self.sequence;
            self.as_mut().rust_mut().last_selection = sequence;
            self.as_mut().set_selected_id(id.clone());
            self.refresh_properties();
        }
    }
    pub fn step(mut self: Pin<&mut Self>, direction: i32) {
        if self.visible.is_empty() {
            return;
        }
        let length = i32::try_from(self.visible.len()).unwrap_or(i32::MAX);
        let row = if *self.selected_row() < 0 {
            0
        } else {
            (*self.selected_row() + direction).clamp(0, length - 1)
        };
        if let Some(&offset) = self.visible.get(row as usize) {
            let id = QString::from(&self.entries[offset].game_id.to_string());
            self.as_mut().select(&id);
        }
    }
    pub fn favorite(self: Pin<&mut Self>, id: &QString, value: bool) {
        if let Ok(id) = Uuid::parse_str(&id.to_string()) {
            self.dispatch(Command::Favorite(id, value), Kind::Other);
        }
    }
    pub fn set_artwork(self: Pin<&mut Self>, id: &QString, url: &QUrl) {
        if !*self.ready() || *self.busy() {
            return;
        }
        let Ok(game) = Uuid::parse_str(&id.to_string()) else {
            return;
        };
        let Some(path) = url.to_local_file() else {
            self.set_error(QString::from("Choose a local artwork file."));
            return;
        };
        if QUrl::from_local_file(&path) != *url {
            self.set_error(QString::from(
                "This artwork URL cannot be represented losslessly as a local path.",
            ));
            return;
        }
        let path = path.to_string();
        if path.starts_with("//") || path.contains('\u{fffd}') {
            self.set_error(QString::from(
                "This artwork path cannot be represented losslessly by the file picker.",
            ));
            return;
        }
        match rustris_core::bytes::OsBytes::new(path.into_bytes()) {
            Ok(path) => {
                self.dispatch(Command::Artwork(game, Some(path)), Kind::Other);
            }
            Err(error) => self.show_error(error, Kind::Other),
        }
    }
    pub fn clear_artwork(self: Pin<&mut Self>, id: &QString) {
        if !*self.ready() || *self.busy() {
            return;
        }
        if let Ok(game) = Uuid::parse_str(&id.to_string()) {
            self.dispatch(Command::Artwork(game, None), Kind::Other);
        }
    }
    pub fn prepare_artwork_path(self: Pin<&mut Self>) {
        self.set_form_error(QString::default());
    }
    pub fn set_artwork_path(self: Pin<&mut Self>, id: &QString, path: &QString, exact: bool) {
        if !*self.ready() || *self.busy() {
            return;
        }
        let Ok(game) = Uuid::parse_str(&id.to_string()) else {
            return;
        };
        match rustris_core::bytes::OsBytes::from_editor(&path.to_string(), exact) {
            Ok(path) if path.path().is_absolute() => {
                self.dispatch(Command::Artwork(game, Some(path)), Kind::Artwork);
            }
            Ok(_) => self.set_form_error(QString::from("Choose an absolute local artwork path.")),
            Err(error) => self.show_error(error, Kind::Artwork),
        }
    }
    pub fn create(mut self: Pin<&mut Self>) {
        if !*self.ready() || *self.busy() {
            return;
        }
        match Draft::default().json() {
            Ok(json) => {
                self.as_mut().set_draft_json(QString::from(&json));
                self.as_mut().set_form_error(QString::default());
                self.open_editor();
            }
            Err(error) => self.show_error(error, Kind::Save),
        }
    }
    pub fn edit(mut self: Pin<&mut Self>, id: &QString) {
        if !*self.ready() || *self.busy() {
            return;
        }
        let value = self
            .entries
            .iter()
            .find(|entry| entry.game_id.to_string() == id.to_string())
            .map(|entry| Draft::from_entry(entry).json());
        match value {
            Some(Ok(json)) => {
                self.as_mut().set_draft_json(QString::from(&json));
                self.as_mut().set_form_error(QString::default());
                self.open_editor();
            }
            Some(Err(error)) => self.show_error(error, Kind::Save),
            None => self.show_error(Error::NotFound, Kind::Save),
        }
    }
    /// D-011: queue one game, or every game without a cover, on the artwork worker.
    fn start_fetch(mut self: Pin<&mut Self>, games: Vec<(Uuid, String)>) {
        if games.is_empty() {
            self.set_artwork_status(QString::from("Every game already has a cover."));
            return;
        }
        let Some(fetcher) = FETCH_CLIENT.get() else {
            self.set_artwork_status(QString::from("The artwork worker is unavailable."));
            return;
        };
        let count = games.len();
        let handle = self.qt_thread();
        let deliver: rustris_core::artwork::Deliver = Box::new(move |game, result| {
            if let Err(error) = handle.queue(move |backend| backend.fetched(game, result)) {
                tracing::debug!(?error, "View closed before artwork delivery");
            }
        });
        let kinds = vec![
            rustris_core::artwork::Kind::Cover,
            rustris_core::artwork::Kind::Hero,
        ];
        match fetcher.submit(games, kinds, deliver) {
            Ok(()) => {
                let pending = {
                    let mut state = self.as_mut().rust_mut();
                    state.fetch_pending += count;
                    state.fetch_pending
                };
                self.as_mut().set_artwork_fetching(true);
                self.set_artwork_status(QString::from(&if pending == 1 {
                    "Downloading artwork…".to_owned()
                } else {
                    format!("Downloading artwork for {pending} games…")
                }));
            }
            Err(error) => self.set_artwork_status(QString::from(&error.to_string())),
        }
    }
    fn fetched(
        mut self: Pin<&mut Self>,
        game: Uuid,
        result: Result<rustris_core::artwork::Outcome>,
    ) {
        let pending = {
            let mut state = self.as_mut().rust_mut();
            state.fetch_pending = state.fetch_pending.saturating_sub(1);
            state.fetch_pending
        };
        self.as_mut().set_artwork_fetching(pending > 0);
        let name = self
            .entries
            .iter()
            .find(|e| e.game_id == game)
            .map_or_else(String::new, |e| e.profile.name.clone());
        let status = match result {
            Ok(outcome) if !outcome.files.is_empty() => {
                self.as_mut()
                    .dispatch(Command::Fetched(game, outcome.files), Kind::Other);
                if pending == 0 {
                    format!("Artwork downloaded for {name}.")
                } else {
                    format!("Artwork downloaded for {name}. {pending} more to go…")
                }
            }
            Ok(outcome) => outcome.note.unwrap_or_default(),
            Err(error) => format!("{name}: {error}"),
        };
        self.set_artwork_status(QString::from(&status));
    }
    pub fn fetch_artwork(self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        let game = self
            .entries
            .iter()
            .find(|e| e.game_id.to_string() == id)
            .map(|e| (e.game_id, e.profile.name.clone()));
        if let Some(game) = game {
            self.start_fetch(vec![game]);
        }
    }
    pub fn fetch_missing_artwork(self: Pin<&mut Self>) {
        let games = self
            .entries
            .iter()
            .filter(|e| e.artwork.is_none() && e.fetched_cover.is_none())
            .map(|e| (e.game_id, e.profile.name.clone()))
            .collect();
        self.start_fetch(games);
    }
    pub fn artwork_settings(
        mut self: Pin<&mut Self>,
        enabled: bool,
        key: &QString,
        clear_key: bool,
    ) {
        use rustris_core::artwork::{Change, KeyChange};
        let key = key.to_string();
        let change = Change {
            enabled,
            key: if clear_key {
                Some(KeyChange::Clear)
            } else if key.trim().is_empty() {
                None
            } else {
                Some(KeyChange::Set(key))
            },
        };
        self.as_mut().set_artwork_status(QString::default());
        self.dispatch(Command::ArtworkSettings(change), Kind::Other);
    }
    pub fn save(self: Pin<&mut Self>, json: &QString) {
        if *self.busy() || !*self.ready() {
            return;
        }
        let result = (|| {
            let draft = Draft::parse(&json.to_string())?;
            let input = draft.input()?;
            Ok(Command::Save(
                draft.id.map(|id| (id, draft.revision)),
                input,
            ))
        })();
        match result {
            Ok(command) => {
                self.dispatch(command, Kind::Save);
            }
            Err(error) => self.show_error(error, Kind::Save),
        }
    }
    pub fn convert(mut self: Pin<&mut Self>, json: &QString, exact: bool) {
        match Draft::parse(&json.to_string())
            .and_then(|draft| draft.converted(exact))
            .and_then(|draft| draft.json())
        {
            Ok(json) => {
                self.as_mut().set_form_error(QString::default());
                self.set_draft_json(QString::from(&json));
            }
            Err(error) => self.show_error(error, Kind::Save),
        }
    }
    pub fn remove(self: Pin<&mut Self>, id: &QString) {
        if let Ok(id) = Uuid::parse_str(&id.to_string()) {
            self.dispatch(Command::Remove(id), Kind::Other);
        }
    }
    pub fn preferences(self: Pin<&mut Self>, opaque: bool, reduced: bool) {
        self.dispatch(
            Command::Preferences(Settings {
                opaque,
                reduced_motion: reduced,
            }),
            Kind::Other,
        );
    }
    pub fn request_close(mut self: Pin<&mut Self>) {
        self.as_mut().set_closing(true);
        if !*self.busy() {
            self.set_close_ready(true);
        }
    }
    pub fn cancel_close(mut self: Pin<&mut Self>) {
        self.as_mut().set_closing(false);
        self.set_close_ready(false);
    }
    pub fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() {
            0
        } else {
            i32::try_from(self.visible.len()).unwrap_or(i32::MAX)
        }
    }
    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut names = QHash::default();
        for (role, name) in [
            (256, "game_id"),
            (257, "title"),
            (258, "favorite"),
            (259, "availability"),
            (260, "runner"),
            (261, "artwork_url"),
        ] {
            names.insert(role, QByteArray::from(name));
        }
        names
    }
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(entry) = usize::try_from(index.row())
            .ok()
            .and_then(|row| self.visible.get(row))
            .and_then(|&offset| self.entries.get(offset))
        else {
            return QVariant::default();
        };
        match role {
            256 => QVariant::from(&QString::from(&entry.game_id.to_string())),
            257 => QVariant::from(&QString::from(&entry.profile.name)),
            258 => QVariant::from(&entry.favorite),
            259 => QVariant::from(&QString::from(&entry.availability)),
            260 => QVariant::from(&QString::from(entry.profile.runner.key())),
            261 => QVariant::from(&QString::from(
                cover(entry)
                    .map(artwork_source)
                    .unwrap_or_default()
                    .as_str(),
            )),
            _ => QVariant::default(),
        }
    }
}
