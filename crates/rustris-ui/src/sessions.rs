#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, busy)]
        #[qproperty(bool, action_busy)]
        #[qproperty(bool, launching)]
        #[qproperty(bool, closing)]
        #[qproperty(bool, close_ready)]
        #[qproperty(i32, active_count)]
        #[qproperty(QString, snapshot_json)]
        #[qproperty(QString, error)]
        #[qproperty(QString, runner_draft_json)]
        #[qproperty(QString, detected_runners_json)]
        type Sessions = super::SessionsRust;
        #[qinvokable]
        fn watch(self: Pin<&mut Sessions>, game: &QString);
        #[qinvokable]
        fn refresh(self: Pin<&mut Sessions>);
        #[qinvokable]
        fn poll(self: Pin<&mut Sessions>);
        #[qinvokable]
        fn choose(self: Pin<&mut Sessions>, session: &QString);
        #[qinvokable]
        fn play(self: Pin<&mut Sessions>, game: &QString, revision: i32);
        #[qinvokable]
        fn cancel_preparation(self: Pin<&mut Sessions>);
        #[qinvokable]
        fn control(self: Pin<&mut Sessions>, session: &QString, action: &QString, generation: i32);
        #[qinvokable]
        fn leave_running(self: Pin<&mut Sessions>);
        #[qinvokable]
        fn load_runner_settings(self: Pin<&mut Sessions>);
        #[qinvokable]
        fn save_runner_settings(self: Pin<&mut Sessions>, json: &QString);
        #[qinvokable]
        fn convert_runner_settings(self: Pin<&mut Sessions>, json: &QString, exact: bool);
        #[qinvokable]
        fn preview_umu(self: Pin<&mut Sessions>, game: &QString, revision: i32);
        #[qinvokable]
        fn confirm_umu(self: Pin<&mut Sessions>, ticket: &QString);
        #[qinvokable]
        fn discard_umu(self: Pin<&mut Sessions>, ticket: &QString);
        #[qsignal]
        fn runner_loaded(self: Pin<&mut Sessions>);
        #[qsignal]
        fn runner_saved(self: Pin<&mut Sessions>);
        #[qsignal]
        fn review_ready(self: Pin<&mut Sessions>);
    }
    impl cxx_qt::Threading for Sessions {}
}
use crate::{RUNTIME, SESSION_CLIENT, runner_draft::RunnerDraft};
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use rustris_core::{
    Error, Result,
    sessions::desktop::{Action, Command, Effect, Snapshot},
};
use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use uuid::Uuid;

#[derive(Default)]
pub struct SessionsRust {
    busy: bool,
    action_busy: bool,
    pending: usize,
    sequence: u64,
    last_sequence: u64,
    launching: bool,
    closing: bool,
    close_ready: bool,
    active_count: i32,
    snapshot_json: QString,
    runner_draft_json: QString,
    detected_runners_json: QString,
    error: QString,
    game: Option<Uuid>,
    selected: Option<Uuid>,
    epoch: u64,
    refresh_needed: bool,
    cancel: Option<Arc<AtomicBool>>,
}
impl ffi::Sessions {
    fn dispatch(mut self: Pin<&mut Self>, command: Command) {
        let (Some(client), Some(runtime)) = (SESSION_CLIENT.get(), RUNTIME.get()) else {
            self.as_mut()
                .set_error(QString::from("The session worker is unavailable."));
            self.as_mut().set_launching(false);
            self.as_mut().rust_mut().cancel = None;
            return;
        };
        let recover = matches!(
            command,
            Command::Play { .. }
                | Command::Control { .. }
                | Command::SaveRunnerSelection(_)
                | Command::PreviewUmu { .. }
                | Command::ConfirmUmu { .. }
        );
        let receiver = match client.submit(command) {
            Ok(receiver) => receiver,
            Err(e) => {
                self.as_mut().set_error(QString::from(&e.to_string()));
                self.as_mut().set_launching(false);
                self.as_mut().rust_mut().cancel = None;
                return;
            }
        };
        {
            let mut state = self.as_mut().rust_mut();
            state.pending += 1;
            state.sequence += 1;
        }
        self.as_mut().set_busy(true);
        if recover {
            self.as_mut().set_action_busy(true);
        }
        let sequence = self.sequence;
        let epoch = self.epoch;
        let handle = self.qt_thread();
        runtime.spawn(async move {
            let result = receiver.await.unwrap_or(Err(Error::WorkerClosed));
            if let Err(e) =
                handle.queue(move |view| view.complete(result, epoch, recover, sequence))
            {
                tracing::debug!(?e, "Session view closed before weak Qt delivery");
            }
        });
    }
    fn complete(
        mut self: Pin<&mut Self>,
        result: Result<Snapshot>,
        epoch: u64,
        recover: bool,
        sequence: u64,
    ) {
        {
            let mut state = self.as_mut().rust_mut();
            debug_assert!(state.pending > 0);
            state.pending -= 1;
        }
        let busy = self.pending > 0;
        self.as_mut().set_busy(busy);
        if recover {
            self.as_mut().set_action_busy(false);
            self.as_mut().set_launching(false);
            self.as_mut().rust_mut().cancel = None;
        }
        match result {
            Ok(mut snapshot) => {
                let effect = std::mem::take(&mut snapshot.effect);
                let review_ready = matches!(&effect, Effect::ReviewReady) && epoch == self.epoch;
                match effect {
                    Effect::RunnerLoaded(selection, detected) => {
                        let detected =
                            serde_json::to_string(&detected).unwrap_or_else(|_| "{}".into());
                        self.as_mut()
                            .set_detected_runners_json(QString::from(&detected));
                        match RunnerDraft::from_selection(selection).json() {
                            Ok(json) => {
                                self.as_mut().set_runner_draft_json(QString::from(&json));
                                self.as_mut().runner_loaded();
                            }
                            Err(e) => self.as_mut().set_error(QString::from(&e.to_string())),
                        }
                    }
                    Effect::RunnerSaved => self.as_mut().runner_saved(),
                    _ => {}
                }
                if sequence >= self.last_sequence {
                    self.as_mut().rust_mut().last_sequence = sequence;
                    self.as_mut().set_active_count(snapshot.active_count as i32);
                }
                if snapshot.close_ready {
                    self.set_close_ready(true);
                    return;
                }
                if epoch == self.epoch && sequence >= self.last_sequence {
                    self.as_mut().rust_mut().selected = snapshot.selected.as_ref().map(|s| s.id);
                    match serde_json::to_string(&snapshot) {
                        Ok(json) => self.as_mut().set_snapshot_json(QString::from(&json)),
                        Err(e) => self.as_mut().set_error(QString::from(&e.to_string())),
                    }
                    if review_ready {
                        self.as_mut().review_ready();
                    }
                }
            }
            Err(e) => {
                self.as_mut().set_error(QString::from(&e.to_string()));
                self.as_mut().set_closing(false);
                if recover {
                    self.as_mut().rust_mut().refresh_needed = true;
                }
            }
        }
        if *self.closing() && !*self.busy() {
            self.dispatch(Command::LeaveRunning);
            return;
        }
        if self.refresh_needed && !*self.busy() {
            self.as_mut().rust_mut().refresh_needed = false;
            self.refresh();
        }
    }
    pub fn watch(mut self: Pin<&mut Self>, game: &QString) {
        let game = Uuid::parse_str(&game.to_string()).ok();
        {
            let mut state = self.as_mut().rust_mut();
            state.game = game;
            state.selected = None;
            state.epoch += 1;
        }
        self.as_mut().set_snapshot_json(QString::from("{}"));
        if *self.busy() {
            self.as_mut().rust_mut().refresh_needed = true;
        } else {
            self.refresh();
        }
    }
    pub fn refresh(mut self: Pin<&mut Self>) {
        // A refresh requested while a poll is in flight runs once it completes,
        // so the Refresh control need not follow the frequent poll state.
        if *self.busy() && !*self.closing() {
            self.as_mut().rust_mut().refresh_needed = true;
        } else {
            self.read(true);
        }
    }
    pub fn poll(self: Pin<&mut Self>) {
        self.read(false);
    }
    fn read(mut self: Pin<&mut Self>, reload_logs: bool) {
        if *self.busy() || *self.closing() {
            return;
        }
        let command = Command::Refresh {
            game: self.game,
            session: self.selected,
            reload_logs,
        };
        self.as_mut().dispatch(command);
    }
    pub fn choose(mut self: Pin<&mut Self>, session: &QString) {
        if *self.closing() {
            return;
        }
        if let Ok(id) = Uuid::parse_str(&session.to_string()) {
            {
                let mut state = self.as_mut().rust_mut();
                state.selected = Some(id);
                state.epoch += 1;
            }
            if *self.busy() {
                self.as_mut().rust_mut().refresh_needed = true;
            } else {
                self.refresh();
            }
        }
    }
    pub fn play(mut self: Pin<&mut Self>, game: &QString, revision: i32) {
        if *self.busy() || *self.closing() {
            return;
        }
        if let Ok(game) = Uuid::parse_str(&game.to_string()) {
            let cancel = Arc::new(AtomicBool::new(false));
            self.as_mut().rust_mut().cancel = Some(cancel.clone());
            self.as_mut().set_error(QString::default());
            self.as_mut().set_launching(true);
            self.dispatch(Command::Play {
                game,
                revision: i64::from(revision),
                cancel,
            });
        }
    }
    pub fn cancel_preparation(mut self: Pin<&mut Self>) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Release);
        }
        self.as_mut().set_error(QString::from("Cancellation requested before spawn. If Start was already accepted, use the separate Stop action."));
    }
    pub fn control(mut self: Pin<&mut Self>, session: &QString, action: &QString, generation: i32) {
        if *self.action_busy() || *self.launching() || *self.closing() {
            return;
        }
        let Ok(session) = Uuid::parse_str(&session.to_string()) else {
            return;
        };
        let action = match action.to_string().as_str() {
            "stop" => Action::Stop,
            "force" => Action::Force,
            "reconnect" => Action::Reconnect,
            "release" => Action::ReleaseUnknown {
                generation: i64::from(generation),
                acknowledged: true,
            },
            _ => return,
        };
        self.as_mut().set_error(QString::default());
        self.dispatch(Command::Control { session, action });
    }
    pub fn load_runner_settings(mut self: Pin<&mut Self>) {
        if *self.action_busy() || *self.closing() {
            return;
        }
        self.as_mut().set_error(QString::default());
        self.dispatch(Command::ReadRunnerSelection);
    }
    pub fn save_runner_settings(mut self: Pin<&mut Self>, json: &QString) {
        if *self.action_busy() || *self.closing() {
            return;
        }
        match RunnerDraft::parse(&json.to_string()).and_then(|d| d.selection()) {
            Ok(selection) => {
                self.as_mut().set_error(QString::default());
                self.dispatch(Command::SaveRunnerSelection(selection));
            }
            Err(e) => self.set_error(QString::from(&e.to_string())),
        }
    }
    pub fn convert_runner_settings(mut self: Pin<&mut Self>, json: &QString, exact: bool) {
        match RunnerDraft::parse(&json.to_string())
            .and_then(|d| d.converted(exact))
            .and_then(|d| d.json())
        {
            Ok(json) => {
                self.as_mut().set_error(QString::default());
                self.set_runner_draft_json(QString::from(&json));
            }
            Err(e) => self.set_error(QString::from(&e.to_string())),
        }
    }
    pub fn preview_umu(mut self: Pin<&mut Self>, game: &QString, revision: i32) {
        if *self.action_busy() || *self.closing() {
            return;
        }
        if let Ok(game) = Uuid::parse_str(&game.to_string()) {
            self.as_mut().set_error(QString::default());
            self.dispatch(Command::PreviewUmu {
                game,
                revision: i64::from(revision),
            });
        }
    }
    pub fn confirm_umu(mut self: Pin<&mut Self>, ticket: &QString) {
        if *self.action_busy() || *self.closing() {
            return;
        }
        if let Ok(ticket) = Uuid::parse_str(&ticket.to_string()) {
            let cancel = Arc::new(AtomicBool::new(false));
            self.as_mut().rust_mut().cancel = Some(cancel.clone());
            self.as_mut().set_error(QString::default());
            self.as_mut().set_launching(true);
            self.dispatch(Command::ConfirmUmu { ticket, cancel });
        }
    }
    pub fn discard_umu(self: Pin<&mut Self>, ticket: &QString) {
        if *self.closing() {
            return;
        }
        if let Ok(ticket) = Uuid::parse_str(&ticket.to_string()) {
            self.dispatch(Command::DiscardUmu { ticket });
        }
    }
    pub fn leave_running(mut self: Pin<&mut Self>) {
        self.as_mut().set_closing(true);
        if !*self.busy() {
            self.dispatch(Command::LeaveRunning);
        }
    }
}
