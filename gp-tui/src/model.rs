#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Portal,
    Username,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Status,
    Profiles,
    Config,
    Logs,
}

impl Page {
    pub fn next_visible(self) -> Self {
        match self {
            Self::Status => Self::Profiles,
            Self::Profiles | Self::Config => Self::Logs,
            Self::Logs => Self::Status,
        }
    }

    pub fn previous_visible(self) -> Self {
        match self {
            Self::Status => Self::Logs,
            Self::Profiles => Self::Status,
            Self::Logs | Self::Config => Self::Profiles,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VpnState {
    NoBackend,
    Disconnected,
    WaitingBrowser,
    WaitingSudo,
    Connecting,
    Connected,
    Disconnecting,
    Error,
    Unknown,
}

impl VpnState {
    pub fn from_snapshot(status: &str) -> Self {
        match status {
            "Connected" => Self::Connected,
            "Disconnected" => Self::Disconnected,
            "Connecting" => Self::Connecting,
            "Error" => Self::Error,
            "Not installed" => Self::NoBackend,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NoBackend => "No backend",
            Self::Disconnected => "Disconnected",
            Self::WaitingBrowser => "Waiting for browser",
            Self::WaitingSudo => "Waiting for sudo",
            Self::Connecting => "Connecting",
            Self::Connected => "Connected",
            Self::Disconnecting => "Disconnecting",
            Self::Error => "Error",
            Self::Unknown => "Unknown",
        }
    }

    pub fn is_busy(self) -> bool {
        matches!(
            self,
            Self::WaitingBrowser | Self::WaitingSudo | Self::Connecting | Self::Disconnecting
        )
    }

    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Connected
                | Self::WaitingBrowser
                | Self::WaitingSudo
                | Self::Connecting
                | Self::Disconnecting
        )
    }
}
