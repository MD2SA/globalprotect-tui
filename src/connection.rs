use crate::model::VpnState;
use std::process::{Child, ExitStatus};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SudoAction {
    Connect,
    Disconnect,
    Quit,
}

pub struct SudoRequest {
    pub action: SudoAction,
    pub cookie: Option<String>,
}

pub struct AuthControl {
    pid: AtomicU32,
    cancelled: AtomicBool,
}

impl AuthControl {
    fn new() -> Self {
        Self {
            pid: AtomicU32::new(0),
            cancelled: AtomicBool::new(false),
        }
    }

    pub fn register_child(&self, pid: u32) -> bool {
        self.pid.store(pid, Ordering::SeqCst);
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn pid(&self) -> u32 {
        self.pid.load(Ordering::Relaxed)
    }

    pub fn complete(&self) {
        self.pid.store(0, Ordering::SeqCst);
    }

    fn cancel(&self) -> Option<u32> {
        self.cancelled.store(true, Ordering::SeqCst);
        let pid = self.pid.swap(0, Ordering::SeqCst);
        (pid > 0).then_some(pid)
    }
}

enum Phase {
    NoBackend,
    Unknown,
    Disconnected,
    OfficialConnecting,
    Authenticating {
        control: Arc<AuthControl>,
        started: Instant,
    },
    WaitingSudo {
        action: SudoAction,
        cookie: Option<String>,
        child: Option<Child>,
    },
    Connecting {
        child: Option<Child>,
    },
    Connected {
        child: Option<Child>,
    },
    Disconnecting {
        child: Option<Child>,
    },
    Failed,
}

pub struct Connection {
    phase: Phase,
    epoch: u64,
    sudo_epoch: u64,
}

impl Connection {
    pub fn new(initial: VpnState) -> Self {
        Self {
            phase: phase_for(initial),
            epoch: 0,
            sudo_epoch: 0,
        }
    }

    pub fn reset(&mut self, initial: VpnState) {
        self.next_epoch();
        self.sudo_epoch = self.sudo_epoch.wrapping_add(1);
        self.phase = phase_for(initial);
    }

    pub fn state(&self) -> VpnState {
        match self.phase {
            Phase::NoBackend => VpnState::NoBackend,
            Phase::Unknown => VpnState::Unknown,
            Phase::Disconnected => VpnState::Disconnected,
            Phase::OfficialConnecting => VpnState::WaitingBrowser,
            Phase::Authenticating { .. } => VpnState::WaitingBrowser,
            Phase::WaitingSudo { .. } => VpnState::WaitingSudo,
            Phase::Connecting { .. } => VpnState::Connecting,
            Phase::Connected { .. } => VpnState::Connected,
            Phase::Disconnecting { .. } => VpnState::Disconnecting,
            Phase::Failed => VpnState::Error,
        }
    }

    pub fn is_active(&self) -> bool {
        self.state().is_active()
    }

    pub fn has_child(&self) -> bool {
        self.child().is_some()
    }

    pub fn auth_started(&self) -> Option<Instant> {
        match self.phase {
            Phase::Authenticating { started, .. } => Some(started),
            _ => None,
        }
    }

    pub fn auth_control(&self) -> Option<Arc<AuthControl>> {
        match &self.phase {
            Phase::Authenticating { control, .. } => Some(Arc::clone(control)),
            _ => None,
        }
    }

    pub fn is_authenticating(&self, epoch: u64) -> bool {
        self.epoch == epoch
            && matches!(
                self.phase,
                Phase::OfficialConnecting | Phase::Authenticating { .. }
            )
    }

    pub fn begin_auth(&mut self) -> (u64, Arc<AuthControl>) {
        self.next_epoch();
        let control = Arc::new(AuthControl::new());
        self.phase = Phase::Authenticating {
            control: Arc::clone(&control),
            started: Instant::now(),
        };
        (self.epoch, control)
    }

    pub fn cancel_auth(&mut self) -> Option<u32> {
        let pid = match &self.phase {
            Phase::Authenticating { control, .. } => control.cancel(),
            Phase::OfficialConnecting => None,
            _ => return None,
        };
        self.next_epoch();
        self.phase = Phase::Disconnected;
        pid
    }

    pub fn begin_official_connect(&mut self) -> u64 {
        self.next_epoch();
        self.phase = Phase::OfficialConnecting;
        self.epoch
    }

    pub fn begin_disconnect(&mut self) -> u64 {
        self.next_epoch();
        let child = self.take_child();
        self.phase = Phase::Disconnecting { child };
        self.epoch
    }

    pub fn begin_waiting_sudo(&mut self, action: SudoAction, cookie: Option<String>) {
        self.next_epoch();
        let child = self.take_child();
        self.phase = Phase::WaitingSudo {
            action,
            cookie,
            child,
        };
    }

    pub fn begin_sudo_auth(&mut self) -> u64 {
        self.sudo_epoch = self.sudo_epoch.wrapping_add(1);
        self.sudo_epoch
    }

    pub fn is_current_sudo_auth(&self, epoch: u64) -> bool {
        self.sudo_epoch == epoch && matches!(self.phase, Phase::WaitingSudo { .. })
    }

    pub fn sudo_action(&self) -> Option<SudoAction> {
        match self.phase {
            Phase::WaitingSudo { action, .. } => Some(action),
            _ => None,
        }
    }

    pub fn cancel_sudo(&mut self) -> Option<SudoAction> {
        let request = self.take_sudo_request()?;
        self.sudo_epoch = self.sudo_epoch.wrapping_add(1);
        let action = request.action;
        self.phase = match action {
            SudoAction::Connect => Phase::Disconnected,
            SudoAction::Disconnect | SudoAction::Quit => Phase::Connected {
                child: self.take_child(),
            },
        };
        Some(action)
    }

    pub fn take_sudo_request(&mut self) -> Option<SudoRequest> {
        let Phase::WaitingSudo {
            action,
            cookie,
            child,
        } = std::mem::replace(&mut self.phase, Phase::Disconnected)
        else {
            return None;
        };
        self.next_epoch();
        self.phase = match action {
            SudoAction::Connect => Phase::Disconnected,
            SudoAction::Disconnect | SudoAction::Quit => Phase::Connected { child },
        };
        Some(SudoRequest { action, cookie })
    }

    pub fn start_gpclient(&mut self, child: Child) {
        self.phase = Phase::Connecting { child: Some(child) };
    }

    pub fn connected(&mut self) {
        let child = self.take_child();
        self.phase = Phase::Connected { child };
    }

    pub fn disconnected(&mut self) {
        self.phase = Phase::Disconnected;
    }

    pub fn failed(&mut self) {
        self.phase = Phase::Failed;
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn is_current_operation(&self, epoch: u64) -> bool {
        self.epoch == epoch
    }

    pub fn reconcile(&mut self, snapshot: VpnState, gpclient_running: bool) {
        let next = reconcile_state(self.state(), snapshot, gpclient_running);
        if next == self.state() {
            return;
        }
        let child = self.take_child();
        self.phase = match next {
            VpnState::NoBackend => Phase::NoBackend,
            VpnState::Unknown => Phase::Unknown,
            VpnState::Disconnected => Phase::Disconnected,
            VpnState::WaitingBrowser => Phase::OfficialConnecting,
            VpnState::WaitingSudo => Phase::WaitingSudo {
                action: SudoAction::Connect,
                cookie: None,
                child,
            },
            VpnState::Connecting => Phase::Connecting { child },
            VpnState::Connected => Phase::Connected { child },
            VpnState::Disconnecting => Phase::Disconnecting { child },
            VpnState::Error => Phase::Failed,
        };
    }

    pub fn reap_child(&mut self) -> Option<ExitStatus> {
        let status = self.child_mut()?.try_wait().ok().flatten()?;
        let phase = std::mem::replace(&mut self.phase, Phase::Unknown);
        self.phase = if !status.success() {
            Phase::Failed
        } else {
            match phase {
                Phase::Connecting { .. } => Phase::Disconnected,
                Phase::Connected { .. } => Phase::Connected { child: None },
                Phase::Disconnecting { .. } => Phase::Disconnecting { child: None },
                Phase::WaitingSudo { action, cookie, .. } => Phase::WaitingSudo {
                    action,
                    cookie,
                    child: None,
                },
                phase => phase,
            }
        };
        Some(status)
    }

    fn next_epoch(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
    }

    fn child(&self) -> Option<&Child> {
        match &self.phase {
            Phase::WaitingSudo { child, .. }
            | Phase::Connecting { child }
            | Phase::Connected { child }
            | Phase::Disconnecting { child } => child.as_ref(),
            _ => None,
        }
    }

    fn child_mut(&mut self) -> Option<&mut Child> {
        match &mut self.phase {
            Phase::WaitingSudo { child, .. }
            | Phase::Connecting { child }
            | Phase::Connected { child }
            | Phase::Disconnecting { child } => child.as_mut(),
            _ => None,
        }
    }

    fn take_child(&mut self) -> Option<Child> {
        match &mut self.phase {
            Phase::WaitingSudo { child, .. }
            | Phase::Connecting { child }
            | Phase::Connected { child }
            | Phase::Disconnecting { child } => child.take(),
            _ => None,
        }
    }
}

fn phase_for(state: VpnState) -> Phase {
    match state {
        VpnState::NoBackend => Phase::NoBackend,
        VpnState::Unknown => Phase::Unknown,
        VpnState::Disconnected => Phase::Disconnected,
        VpnState::WaitingBrowser => Phase::OfficialConnecting,
        VpnState::Connecting => Phase::Connecting { child: None },
        VpnState::Connected => Phase::Connected { child: None },
        VpnState::Disconnecting => Phase::Disconnecting { child: None },
        VpnState::WaitingSudo => Phase::WaitingSudo {
            action: SudoAction::Connect,
            cookie: None,
            child: None,
        },
        VpnState::Error => Phase::Failed,
    }
}

fn reconcile_state(current: VpnState, snapshot: VpnState, gpclient_running: bool) -> VpnState {
    // A poll cannot complete an in-flight authentication or disconnect action.
    let next = match (current, snapshot) {
        (VpnState::WaitingBrowser, _) => VpnState::WaitingBrowser,
        (VpnState::WaitingSudo, _) => VpnState::WaitingSudo,
        (VpnState::Disconnecting, _) => VpnState::Disconnecting,
        (VpnState::Error, VpnState::Disconnected) => VpnState::Error,
        (_, state) => state,
    };
    if gpclient_running && next == VpnState::Disconnected {
        VpnState::Connecting
    } else {
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn keeps_local_operations_when_snapshots_lag() {
        let mut connection = Connection::new(VpnState::Disconnected);
        connection.begin_auth();
        connection.reconcile(VpnState::Disconnected, false);
        assert_eq!(connection.state(), VpnState::WaitingBrowser);

        connection.begin_waiting_sudo(SudoAction::Connect, Some("cookie".to_string()));
        let sudo_epoch = connection.begin_sudo_auth();
        connection.reconcile(VpnState::Connected, false);
        assert_eq!(connection.state(), VpnState::WaitingSudo);
        assert!(connection.is_current_sudo_auth(sudo_epoch));

        connection.connected();
        connection.begin_disconnect();
        connection.reconcile(VpnState::Disconnected, false);
        assert_eq!(connection.state(), VpnState::Disconnecting);

        connection.failed();
        connection.reconcile(VpnState::Disconnected, false);
        assert_eq!(connection.state(), VpnState::Error);
    }

    #[test]
    fn sudo_request_retains_cookie_and_restores_disconnect_child() {
        let mut connection = Connection::new(VpnState::Disconnected);
        let child = Command::new("true").spawn().expect("spawn test child");
        connection.start_gpclient(child);
        connection.begin_waiting_sudo(SudoAction::Disconnect, None);

        let request = connection.take_sudo_request().expect("sudo request");
        assert_eq!(request.action, SudoAction::Disconnect);
        assert_eq!(connection.state(), VpnState::Connected);
        assert!(connection.has_child());

        connection.begin_waiting_sudo(SudoAction::Connect, Some("cookie".to_string()));
        let request = connection.take_sudo_request().expect("sudo request");
        assert_eq!(request.action, SudoAction::Connect);
        assert_eq!(request.cookie.as_deref(), Some("cookie"));
        assert_eq!(connection.state(), VpnState::Disconnected);
    }

    #[test]
    fn cancelling_auth_invalidates_its_result_and_signals_only_its_pid() {
        let mut connection = Connection::new(VpnState::Disconnected);
        let (epoch, control) = connection.begin_auth();
        assert!(!control.register_child(123));

        assert_eq!(connection.cancel_auth(), Some(123));
        assert!(!connection.is_authenticating(epoch));
        assert_eq!(connection.state(), VpnState::Disconnected);
    }

    #[test]
    fn cancellation_before_child_spawn_is_observed_by_the_worker() {
        let mut connection = Connection::new(VpnState::Disconnected);
        let (epoch, control) = connection.begin_auth();

        assert_eq!(connection.cancel_auth(), None);
        assert!(control.register_child(456));
        control.complete();

        assert!(!connection.is_authenticating(epoch));
        assert_eq!(control.pid(), 0);
    }

    #[test]
    fn transitions_advance_the_connection_generation() {
        let mut connection = Connection::new(VpnState::Disconnected);
        let connect_epoch = connection.begin_official_connect();
        let disconnect_epoch = connection.begin_disconnect();

        assert_ne!(connect_epoch, disconnect_epoch);
        assert!(!connection.is_current_operation(connect_epoch));
        assert!(connection.is_current_operation(disconnect_epoch));
    }
}
