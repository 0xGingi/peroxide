use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use ssh2::Session;
use std::fmt;
use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};
mod file_browser;
pub use file_browser::FileBrowser;

const CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);

/// Form fields in display order: name, host, port, user, key, passphrase, password, forward.
pub const FORM_FIELDS: usize = 8;
pub const PORT_FIELD: usize = 2;
pub const KEY_FIELD: usize = 4;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum InputMode {
    Normal,
    Search,
    Adding,
    Editing,
    ConfirmDelete,
    Keys,
    FileBrowser,
    Themes,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    Info(String),
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConnection {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub key_path: Option<PathBuf>,
    pub key_passphrase: Option<String>,
    #[serde(default)]
    pub local_forward: Option<String>,
    #[serde(skip)]
    pub last_connection_status: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct FormState {
    pub name: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub password: String,
    pub key_passphrase: String,
    pub local_forward: String,
    /// 0 means no key, otherwise an index into `ssh_keys` plus one.
    pub selected_key: usize,
    pub active_field: usize,
}

/// Preferences saved between runs.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Theme name; empty or unknown means the default theme.
    #[serde(default)]
    pub theme: String,
}

/// A connection check running on a background thread.
#[derive(Debug)]
pub struct Pending {
    pub idx: usize,
    pub connect: bool,
    pub started: Instant,
    rx: Receiver<Result<(), AppError>>,
}

#[derive(Debug)]
pub struct App {
    pub connections: Vec<SshConnection>,
    pub ssh_keys: Vec<PathBuf>,
    pub additional_key_paths: Vec<PathBuf>,
    pub selected_connection: Option<usize>,
    pub input_mode: InputMode,
    pub form_state: FormState,
    pub notice: Option<Notice>,
    pub keys_selected: usize,
    pub file_browser: Option<FileBrowser>,
    pub search_query: String,
    pub search_selected: Option<usize>,
    pub pending: Option<Pending>,
    pub settings: Settings,
    /// Row under the cursor in the theme picker.
    pub theme_cursor: usize,
}

#[derive(Debug)]
pub enum AppError {
    ConnectionFailed(String),
    AuthenticationFailed(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            AppError::AuthenticationFailed(msg) => write!(f, "Authentication failed: {}", msg),
        }
    }
}

/// True for files in a key folder that could be private keys.
pub fn is_key_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    path.is_file()
        && !name.contains("known_hosts")
        && !name.contains("authorized_keys")
        && !name.contains("config")
        && !name.ends_with(".pub")
        && !name.starts_with('.')
}

/// Shortens paths under the home directory to `~/...`.
pub fn display_path(path: &Path) -> String {
    match dirs::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

fn config_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("Could not find config directory")?
        .join("peroxide");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Writes to a temp file first so a crash mid-write can't truncate saved data.
fn write_config(name: &str, content: String) -> Result<()> {
    let path = config_dir()?.join(name);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, content)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn read_config<T: serde::de::DeserializeOwned + Default>(name: &str) -> Result<T> {
    let path = config_dir()?.join(name);
    if !path.exists() {
        return Ok(T::default());
    }
    let content = fs::read_to_string(&path)?;
    serde_json::from_str(&content).with_context(|| format!("Could not parse {}", path.display()))
}

impl SshConnection {
    /// `user@host`, plus `:port` when it isn't 22.
    pub fn address(&self) -> String {
        if self.port == 22 {
            format!("{}@{}", self.username, self.host)
        } else {
            format!("{}@{}:{}", self.username, self.host, self.port)
        }
    }

    /// Arguments passed to the system `ssh` binary.
    pub fn ssh_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if self.port != 22 {
            args.extend(["-p".to_string(), self.port.to_string()]);
        }
        if let Some(key) = &self.key_path {
            args.extend(["-i".to_string(), key.to_string_lossy().into_owned()]);
        }
        if let Some(fwd) = &self.local_forward {
            args.extend(["-L".to_string(), fwd.clone()]);
        }
        args.extend(["-o".to_string(), "StrictHostKeyChecking=no".to_string()]);
        args.push(format!("{}@{}", self.username, self.host));
        args
    }

    /// The `ssh` command, wrapped in sshpass when a stored secret has to be typed.
    /// The secret goes through the SSHPASS env var so it never shows up in `ps`.
    pub fn command(&self) -> Command {
        let secret = match (&self.key_path, &self.key_passphrase, &self.password) {
            (Some(_), Some(passphrase), _) => Some((Some("Enter passphrase for key"), passphrase)),
            (None, _, Some(password)) => Some((None, password)),
            _ => None,
        };
        let mut cmd = match secret {
            Some((prompt, secret)) => {
                let mut cmd = Command::new("sshpass");
                if let Some(prompt) = prompt {
                    cmd.args(["-P", prompt]);
                }
                cmd.arg("-e").env("SSHPASS", secret).arg("ssh");
                cmd
            }
            None => Command::new("ssh"),
        };
        cmd.args(self.ssh_args()).env("TERM", "xterm-256color");
        cmd
    }

    /// Opens a session and signs in without starting a shell.
    pub fn check(&self) -> Result<(), AppError> {
        let fail = |e: ssh2::Error| AppError::ConnectionFailed(e.to_string());
        let tcp = connect_tcp(&self.host, self.port)?;
        let mut sess = Session::new().map_err(fail)?;
        sess.set_tcp_stream(tcp);
        sess.handshake().map_err(fail)?;

        let auth = if let Some(key_path) = &self.key_path {
            sess.userauth_pubkey_file(
                &self.username,
                None,
                key_path,
                self.key_passphrase.as_deref(),
            )
        } else if let Some(password) = &self.password {
            sess.userauth_password(&self.username, password)
        } else {
            sess.userauth_agent(&self.username)
        };
        auth.map_err(|e| AppError::AuthenticationFailed(e.to_string()))
    }
}

fn connect_tcp(host: &str, port: u16) -> Result<TcpStream, AppError> {
    let addrs = (host, port)
        .to_socket_addrs()
        .map_err(|_| AppError::ConnectionFailed(format!("can't resolve {host}")))?;

    let mut last_error = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, CONNECTION_TIMEOUT) {
            Ok(tcp) => {
                let timeout = Some(CONNECTION_TIMEOUT);
                tcp.set_read_timeout(timeout)
                    .and_then(|_| tcp.set_write_timeout(timeout))
                    .map_err(|e| {
                        AppError::ConnectionFailed(format!("Failed to set timeout: {}", e))
                    })?;
                return Ok(tcp);
            }
            Err(e) => last_error = Some(e),
        }
    }

    Err(AppError::ConnectionFailed(format!(
        "no answer within {} seconds: {}",
        CONNECTION_TIMEOUT.as_secs(),
        last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "no resolved addresses".to_string())
    )))
}

impl FormState {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            host: String::new(),
            port: String::from("22"),
            username: String::new(),
            password: String::new(),
            key_passphrase: String::new(),
            local_forward: String::new(),
            selected_key: 0,
            active_field: 0,
        }
    }

    fn text_mut(&mut self, field: usize) -> Option<&mut String> {
        Some(match field {
            0 => &mut self.name,
            1 => &mut self.host,
            2 => &mut self.port,
            3 => &mut self.username,
            5 => &mut self.key_passphrase,
            6 => &mut self.password,
            7 => &mut self.local_forward,
            _ => return None,
        })
    }
}

impl Default for FormState {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let mut ssh_keys: Vec<PathBuf> = dirs::home_dir()
            .and_then(|home| fs::read_dir(home.join(".ssh")).ok())
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| is_key_file(path))
            .collect();
        ssh_keys.sort();

        Self {
            connections: Vec::new(),
            ssh_keys,
            additional_key_paths: Vec::new(),
            selected_connection: None,
            input_mode: InputMode::Normal,
            form_state: FormState::new(),
            notice: None,
            keys_selected: 0,
            file_browser: None,
            search_query: String::new(),
            search_selected: None,
            pending: None,
            settings: Settings::default(),
            theme_cursor: 0,
        }
    }

    pub fn selected(&self) -> Option<&SshConnection> {
        self.selected_connection
            .and_then(|idx| self.connections.get(idx))
    }

    pub fn info(&mut self, message: impl Into<String>) {
        self.notice = Some(Notice::Info(message.into()));
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.notice = Some(Notice::Error(message.into()));
    }

    // List

    /// Moves the list cursor, or the filtered cursor while searching.
    pub fn move_selection(&mut self, down: bool) {
        let len = if self.input_mode == InputMode::Search {
            self.filtered_indices().len()
        } else {
            self.connections.len()
        };
        let selection = if self.input_mode == InputMode::Search {
            &mut self.search_selected
        } else {
            &mut self.selected_connection
        };
        *selection = match *selection {
            _ if len == 0 => None,
            Some(i) if down => Some((i + 1).min(len - 1)),
            Some(i) => Some(i.saturating_sub(1).min(len - 1)),
            None => Some(0),
        };
    }

    pub fn move_selected_up(&mut self) {
        if let Some(idx) = self.selected_connection {
            if idx > 0 && idx < self.connections.len() {
                self.connections.swap(idx, idx - 1);
                self.selected_connection = Some(idx - 1);
            }
        }
    }

    pub fn move_selected_down(&mut self) {
        if let Some(idx) = self.selected_connection {
            if idx + 1 < self.connections.len() {
                self.connections.swap(idx, idx + 1);
                self.selected_connection = Some(idx + 1);
            }
        }
    }

    pub fn duplicate_selected(&mut self) -> Option<String> {
        let mut copy = self.selected()?.clone();
        copy.name = format!("{} (copy)", copy.name);
        copy.last_connection_status = None;
        let name = copy.name.clone();
        self.connections.push(copy);
        self.selected_connection = Some(self.connections.len() - 1);
        Some(name)
    }

    pub fn delete_selected(&mut self) -> Option<SshConnection> {
        let idx = self
            .selected_connection
            .filter(|&idx| idx < self.connections.len())?;
        let removed = self.connections.remove(idx);
        self.selected_connection = match self.connections.len() {
            0 => None,
            len => Some(idx.min(len - 1)),
        };
        Some(removed)
    }

    // Search

    pub fn filtered_indices(&self) -> Vec<usize> {
        let q = self.search_query.trim().to_lowercase();
        self.connections
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                q.is_empty()
                    || [
                        c.name.as_str(),
                        c.username.as_str(),
                        c.host.as_str(),
                        &c.port.to_string(),
                        &c.key_path
                            .as_deref()
                            .map(|k| k.to_string_lossy())
                            .unwrap_or_default(),
                        c.local_forward.as_deref().unwrap_or(""),
                    ]
                    .iter()
                    .any(|field| field.to_lowercase().contains(&q))
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn start_search(&mut self) {
        self.search_query.clear();
        self.input_mode = InputMode::Search;
        self.search_selected = self.selected_connection.or(Some(0));
        if self.connections.is_empty() {
            self.search_selected = None;
        }
    }

    /// Adds a character to the query, or removes the last one for `None`.
    pub fn edit_search(&mut self, c: Option<char>) {
        match c {
            Some(c) => self.search_query.push(c),
            None => {
                self.search_query.pop();
            }
        }
        self.search_selected = if self.filtered_indices().is_empty() {
            None
        } else {
            Some(0)
        };
    }

    /// Leaves search, moving the list cursor to the match when `keep` is set.
    pub fn end_search(&mut self, keep: bool) {
        if keep {
            let filtered = self.filtered_indices();
            if let Some(&idx) = self.search_selected.and_then(|sel| filtered.get(sel)) {
                self.selected_connection = Some(idx);
            }
        }
        self.search_query.clear();
        self.search_selected = None;
        self.input_mode = InputMode::Normal;
    }

    // Form

    pub fn start_adding(&mut self) {
        self.form_state = FormState::new();
        self.input_mode = InputMode::Adding;
    }

    pub fn edit_connection(&mut self) {
        let Some(conn) = self.selected().cloned() else {
            return;
        };
        // A key that was removed from the list must survive an edit, so put it back.
        let selected_key = match &conn.key_path {
            Some(path) => {
                let pos = self.ssh_keys.iter().position(|p| p == path);
                pos.unwrap_or_else(|| {
                    self.ssh_keys.push(path.clone());
                    self.ssh_keys.len() - 1
                }) + 1
            }
            None => 0,
        };

        self.form_state = FormState {
            name: conn.name,
            host: conn.host,
            port: conn.port.to_string(),
            username: conn.username,
            password: conn.password.unwrap_or_default(),
            key_passphrase: conn.key_passphrase.unwrap_or_default(),
            local_forward: conn.local_forward.unwrap_or_default(),
            selected_key,
            active_field: 0,
        };
        self.input_mode = InputMode::Editing;
    }

    pub fn add_char(&mut self, c: char) {
        let field = self.form_state.active_field;
        if field == PORT_FIELD && !c.is_ascii_digit() {
            return;
        }
        if let Some(text) = self.form_state.text_mut(field) {
            text.push(c);
        }
    }

    pub fn delete_char(&mut self) {
        if let Some(text) = self.form_state.text_mut(self.form_state.active_field) {
            text.pop();
        }
    }

    pub fn next_field(&mut self) {
        self.form_state.active_field = (self.form_state.active_field + 1) % FORM_FIELDS;
    }

    pub fn previous_field(&mut self) {
        self.form_state.active_field =
            (self.form_state.active_field + FORM_FIELDS - 1) % FORM_FIELDS;
    }

    pub fn select_ssh_key(&mut self, direction: i32) {
        let total = self.ssh_keys.len() as i32 + 1;
        self.form_state.selected_key =
            (self.form_state.selected_key as i32 + direction).rem_euclid(total) as usize;
    }

    /// Saves the form as a new connection, or over the one being edited.
    pub fn submit_form(&mut self) -> Result<String, &'static str> {
        let connection = self.connection_from_form()?;
        let name = connection.name.clone();
        match self.selected_connection {
            Some(idx) if self.input_mode == InputMode::Editing && idx < self.connections.len() => {
                self.connections[idx] = connection;
            }
            _ => {
                self.connections.push(connection);
                self.selected_connection = Some(self.connections.len() - 1);
            }
        }
        self.input_mode = InputMode::Normal;
        Ok(name)
    }

    fn connection_from_form(&self) -> Result<SshConnection, &'static str> {
        let form = &self.form_state;
        if form.name.trim().is_empty()
            || form.host.trim().is_empty()
            || form.username.trim().is_empty()
        {
            return Err("Name, host and user are required");
        }

        let non_empty = |s: &str| (!s.is_empty()).then(|| s.to_string());

        Ok(SshConnection {
            name: form.name.trim().to_string(),
            host: form.host.trim().to_string(),
            port: Self::parse_port(&form.port)?,
            username: form.username.trim().to_string(),
            password: non_empty(&form.password),
            key_path: form
                .selected_key
                .checked_sub(1)
                .and_then(|idx| self.ssh_keys.get(idx).cloned()),
            key_passphrase: non_empty(&form.key_passphrase),
            local_forward: Self::parse_local_forward(&form.local_forward)?,
            last_connection_status: None,
        })
    }

    fn parse_port(value: &str) -> Result<u16, &'static str> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err("Port is required");
        }

        match trimmed.parse::<u16>() {
            Ok(port) if port > 0 => Ok(port),
            _ => Err("Port must be a number from 1 to 65535"),
        }
    }

    fn parse_local_forward(value: &str) -> Result<Option<String>, &'static str> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        let parts: Vec<&str> = trimmed.split(':').collect();
        if parts.len() != 3 {
            return Err("Forward must look like local_port:host:remote_port");
        }

        Self::parse_port(parts[0]).map_err(|_| "Forward has an invalid local port")?;
        if parts[1].trim().is_empty() || parts[1].chars().any(char::is_whitespace) {
            return Err("Forward host can't be empty or contain spaces");
        }
        Self::parse_port(parts[2]).map_err(|_| "Forward has an invalid remote port")?;

        Ok(Some(format!(
            "{}:{}:{}",
            parts[0].trim(),
            parts[1].trim(),
            parts[2].trim()
        )))
    }

    // Checks

    /// Checks the selected server on a background thread. `connect` opens ssh if it passes.
    pub fn start_check(&mut self, connect: bool) {
        if self.pending.is_some() {
            return;
        }
        let Some(idx) = self
            .selected_connection
            .filter(|&idx| idx < self.connections.len())
        else {
            self.error("Select a server first");
            return;
        };
        let conn = self.connections[idx].clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(conn.check());
        });
        self.pending = Some(Pending {
            idx,
            connect,
            started: Instant::now(),
            rx,
        });
    }

    /// Returns the finished check, if there is one, and records its result.
    pub fn poll_check(&mut self) -> Option<(usize, bool, Result<(), AppError>)> {
        let result = match self.pending.as_ref()?.rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(AppError::ConnectionFailed(
                "the check stopped unexpectedly".to_string(),
            )),
        };
        let pending = self.pending.take()?;
        if let Some(conn) = self.connections.get_mut(pending.idx) {
            conn.last_connection_status = Some(result.is_ok());
        }
        Some((pending.idx, pending.connect, result))
    }

    // Keys

    /// Returns true if the key was new.
    pub fn add_key_path(&mut self, path: PathBuf) -> bool {
        if path.is_file() && !self.ssh_keys.contains(&path) {
            self.additional_key_paths.push(path.clone());
            self.ssh_keys.push(path);
            true
        } else {
            false
        }
    }

    pub fn remove_ssh_key(&mut self, index: usize) {
        if index < self.ssh_keys.len() {
            let path = self.ssh_keys.remove(index);
            self.additional_key_paths.retain(|p| p != &path);
            self.keys_selected = self
                .keys_selected
                .min(self.ssh_keys.len().saturating_sub(1));
        }
    }

    pub fn open_file_browser(&mut self, pick_folder: bool) {
        let home = dirs::home_dir().unwrap_or_default();
        let ssh = home.join(".ssh");
        let start = if ssh.is_dir() { ssh } else { home };
        self.file_browser = Some(FileBrowser::new(start, pick_folder));
        self.input_mode = InputMode::FileBrowser;
    }

    pub fn close_file_browser(&mut self) {
        self.file_browser = None;
        self.input_mode = InputMode::Keys;
    }

    /// Enter in the file browser: open a folder, or add the chosen key or folder of keys.
    pub fn browser_select(&mut self) {
        let Some(browser) = &mut self.file_browser else {
            return;
        };
        let Some(path) = browser.get_selected_path() else {
            return;
        };
        let use_folder = browser.pick_folder && path == browser.current_path;

        if use_folder {
            let found: Vec<PathBuf> = fs::read_dir(&path)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|p| is_key_file(p))
                .collect();
            let added = found
                .into_iter()
                .filter(|p| self.add_key_path(p.clone()))
                .count();
            let plural = if added == 1 { "" } else { "s" };
            self.info(format!(
                "Added {added} key{plural} from {}",
                display_path(&path)
            ));
        } else if path.is_dir() {
            browser.enter_directory();
            return;
        } else if is_key_file(&path) {
            let name = display_path(&path);
            if self.add_key_path(path) {
                self.info(format!("Added {name}"));
            } else {
                self.info(format!("{name} is already in your keys"));
            }
        } else {
            self.error(format!(
                "{} doesn't look like a private key",
                display_path(&path)
            ));
            return;
        }

        if let Err(e) = self.save_additional_keys() {
            self.error(format!("Couldn't save keys: {e}"));
        }
        self.close_file_browser();
    }

    // Storage

    pub fn load_connections() -> Result<Vec<SshConnection>> {
        read_config("connections.json")
    }

    pub fn save_connections(&self) -> Result<()> {
        write_config(
            "connections.json",
            serde_json::to_string_pretty(&self.connections)?,
        )
    }

    pub fn load_settings() -> Result<Settings> {
        read_config("settings.json")
    }

    pub fn save_settings(&self) -> Result<()> {
        write_config(
            "settings.json",
            serde_json::to_string_pretty(&self.settings)?,
        )
    }

    pub fn load_additional_keys() -> Result<Vec<PathBuf>> {
        read_config("additional_keys.json")
    }

    pub fn save_additional_keys(&self) -> Result<()> {
        write_config(
            "additional_keys.json",
            serde_json::to_string_pretty(&self.additional_key_paths)?,
        )
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{App, SshConnection};
    use std::path::PathBuf;

    #[test]
    fn parse_port_rejects_empty_zero_and_out_of_range_values() {
        assert!(App::parse_port("").is_err());
        assert!(App::parse_port("0").is_err());
        assert!(App::parse_port("65536").is_err());
        assert_eq!(App::parse_port("22"), Ok(22));
    }

    #[test]
    fn parse_local_forward_accepts_expected_format() {
        assert_eq!(
            App::parse_local_forward(" 8080:127.0.0.1:80 "),
            Ok(Some("8080:127.0.0.1:80".to_string()))
        );
    }

    #[test]
    fn parse_local_forward_rejects_invalid_format() {
        assert!(App::parse_local_forward("8080").is_err());
        assert!(App::parse_local_forward("0:127.0.0.1:80").is_err());
        assert!(App::parse_local_forward("8080::80").is_err());
        assert!(App::parse_local_forward("8080:bad host:80").is_err());
        assert!(App::parse_local_forward("8080:127.0.0.1:0").is_err());
    }

    #[test]
    fn passphrase_command_keeps_port_and_hides_secret() {
        let conn = SshConnection {
            name: "box".into(),
            host: "example.com".into(),
            port: 2222,
            username: "me".into(),
            password: None,
            key_path: Some(PathBuf::from("/k/id_ed25519")),
            key_passphrase: Some("hunter2".into()),
            local_forward: None,
            last_connection_status: None,
        };
        let cmd = conn.command();
        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy()).collect();
        assert_eq!(cmd.get_program(), "sshpass");
        assert!(args.windows(2).any(|w| w[0] == "-p" && w[1] == "2222"));
        assert!(!args.iter().any(|a| a.contains("hunter2")));
        assert!(cmd
            .get_envs()
            .any(|(k, v)| k == "SSHPASS" && v == Some("hunter2".as_ref())));
    }

    #[test]
    fn editing_keeps_a_key_missing_from_the_list() {
        let mut app = App::new();
        app.ssh_keys.clear();
        app.connections.push(SshConnection {
            name: "box".into(),
            host: "example.com".into(),
            port: 22,
            username: "me".into(),
            password: None,
            key_path: Some(PathBuf::from("/gone/id_rsa")),
            key_passphrase: None,
            local_forward: None,
            last_connection_status: None,
        });
        app.selected_connection = Some(0);
        app.edit_connection();
        app.submit_form().unwrap();
        assert_eq!(
            app.connections[0].key_path,
            Some(PathBuf::from("/gone/id_rsa"))
        );
    }
}
