use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use anyhow::{Result, Context};
use ssh2::Session;
use std::net::TcpStream;
use std::process::Command;
use std::fmt;
use crossterm::terminal::{enable_raw_mode, disable_raw_mode};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{Clear, ClearType};
use std::io::Write;
use std::thread;
use std::time::Duration;
mod file_browser;
use file_browser::FileBrowser;

#[derive(Debug, PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
    Adding,
    Settings,
    FileBrowser(FileBrowserMode),
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum FileBrowserMode {
    SingleFile,
    Directory,
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
    #[serde(skip)]
    pub last_connection_status: Option<bool>,
}

#[derive(Debug)]
pub enum SettingsTab {
    SshKeys,
}

#[derive(Debug, Clone)]
pub struct FormState {
    pub name: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub password: String,
    pub key_passphrase: String,
    pub selected_key: Option<usize>,
    pub active_field: usize,
}

#[derive(Debug)]
pub struct App {
    pub connections: Vec<SshConnection>,
    pub ssh_keys: Vec<PathBuf>,
    pub additional_key_paths: Vec<PathBuf>,
    pub selected_connection: Option<usize>,
    pub input_mode: InputMode,
    pub form_state: FormState,
    pub error_message: Option<String>,
    pub settings_tab: SettingsTab,
    pub settings_selected_item: usize,
    pub file_browser: Option<FileBrowser>,
}

#[derive(Debug)]
pub enum AppError {
    ConnectionFailed(String),
    AuthenticationFailed(String),
    NoConnectionSelected,
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            AppError::AuthenticationFailed(msg) => write!(f, "Authentication failed: {}", msg),
            AppError::NoConnectionSelected => write!(f, "No connection selected"),
        }
    }
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
            selected_key: None,
            active_field: 0,
        }
    }
}

impl App {
    pub fn new() -> Self {
        let mut ssh_keys = Vec::new();
        if let Some(home) = dirs::home_dir() {
            let ssh_dir = home.join(".ssh");
            if let Ok(entries) = std::fs::read_dir(ssh_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        let file_name = path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("");
                        
                        if !file_name.contains("known_hosts") &&
                           !file_name.contains("authorized_keys") &&
                           !file_name.contains("config") &&
                           !file_name.ends_with(".pub") &&
                           !file_name.starts_with(".") {
                            ssh_keys.push(path);
                        }
                    }
                }
            }
        }

        Self {
            connections: Vec::new(),
            ssh_keys,
            additional_key_paths: Vec::new(),
            selected_connection: None,
            input_mode: InputMode::Normal,
            form_state: FormState::new(),
            error_message: None,
            settings_tab: SettingsTab::SshKeys,
            settings_selected_item: 0,
            file_browser: None,
        }
    }

    pub fn add_char(&mut self, c: char) {
        match self.form_state.active_field {
            0 => self.form_state.name.push(c),
            1 => self.form_state.host.push(c),
            2 => {
                if c.is_ascii_digit() && self.form_state.port.len() < 5 {
                    self.form_state.port.push(c);
                }
            }
            3 => self.form_state.username.push(c),
            4 => self.form_state.password.push(c),
            5 => self.form_state.key_passphrase.push(c),
            _ => {}
        }
    }

    pub fn delete_char(&mut self) {
        match self.form_state.active_field {
            0 => { self.form_state.name.pop(); }
            1 => { self.form_state.host.pop(); }
            2 => { self.form_state.port.pop(); }
            3 => { self.form_state.username.pop(); }
            4 => { self.form_state.password.pop(); }
            5 => { self.form_state.key_passphrase.pop(); }
            _ => {}
        }
    }

    pub fn next_field(&mut self) {
        self.form_state.active_field = (self.form_state.active_field + 1) % 7;
    }

    pub fn previous_field(&mut self) {
        if self.form_state.active_field == 0 {
            self.form_state.active_field = 6;
        } else {
            self.form_state.active_field -= 1;
        }
    }

    pub fn save_connection(&mut self) -> Result<(), &'static str> {
        if self.form_state.name.is_empty() || self.form_state.host.is_empty() || self.form_state.username.is_empty() {
            return Err("Required fields cannot be empty");
        }

        let port = self.form_state.port.parse().unwrap_or(22);
        if port == 0 {
            return Err("Invalid port number");
        }

        let key_path = self.form_state.selected_key.map(|idx| self.ssh_keys[idx].clone());
        let password = if self.form_state.password.is_empty() {
            None
        } else {
            Some(self.form_state.password.clone())
        };
        
        let key_passphrase = if self.form_state.key_passphrase.is_empty() {
            None
        } else {
            Some(self.form_state.key_passphrase.clone())
        };

        let connection = SshConnection {
            name: self.form_state.name.clone(),
            host: self.form_state.host.clone(),
            port,
            username: self.form_state.username.clone(),
            password,
            key_path,
            key_passphrase,
            last_connection_status: None,
        };

        self.connections.push(connection);
        Ok(())
    }

    pub fn load_connections() -> Result<Vec<SshConnection>> {
        let config_dir = dirs::config_dir()
            .context("Could not find config directory")?
            .join("peroxide");
        
        fs::create_dir_all(&config_dir)?;
        let config_file = config_dir.join("connections.json");
        
        if !config_file.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(config_file)?;
        let connections = serde_json::from_str(&content)?;
        Ok(connections)
    }

    pub fn save_connections(&self) -> Result<()> {
        let config_dir = dirs::config_dir()
            .context("Could not find config directory")?
            .join("peroxide");
        
        fs::create_dir_all(&config_dir)?;
        let config_file = config_dir.join("connections.json");
        
        let content = serde_json::to_string_pretty(&self.connections)?;
        fs::write(config_file, content)?;
        Ok(())
    }

    pub fn edit_connection(&mut self) {
        if let Some(idx) = self.selected_connection {
            let connection_data = if let Some(conn) = self.connections.get(idx) {
                let selected_key = if let Some(key_path) = &conn.key_path {
                    self.ssh_keys.iter().position(|p| p == key_path)
                } else {
                    None
                };

                Some((
                    conn.name.clone(),
                    conn.host.clone(),
                    conn.port.to_string(),
                    conn.username.clone(),
                    conn.password.clone().unwrap_or_default(),
                    conn.key_passphrase.clone().unwrap_or_default(),
                    selected_key,
                ))
            } else {
                None
            };

            if let Some((name, host, port, username, password, key_passphrase, selected_key)) = connection_data {
                self.form_state = FormState {
                    name,
                    host,
                    port,
                    username,
                    password,
                    key_passphrase,
                    selected_key,
                    active_field: 0,
                };
                self.input_mode = InputMode::Editing;
            }
        }
    }

    pub fn update_connection(&mut self) -> Result<(), &'static str> {
        if let Some(idx) = self.selected_connection {
            if self.form_state.name.is_empty() || self.form_state.host.is_empty() || self.form_state.username.is_empty() {
                return Err("Required fields cannot be empty");
            }

            let port = self.form_state.port.parse().unwrap_or(22);
            if port == 0 {
                return Err("Invalid port number");
            }

            let key_path = self.form_state.selected_key.map(|idx| {
                let path = self.ssh_keys[idx].clone();
                path
            });

            let password = if self.form_state.password.is_empty() {
                None
            } else {
                Some(self.form_state.password.clone())
            };
            
            let key_passphrase = if self.form_state.key_passphrase.is_empty() {
                None
            } else {
                Some(self.form_state.key_passphrase.clone())
            };

            let connection = SshConnection {
                name: self.form_state.name.clone(),
                host: self.form_state.host.clone(),
                port,
                username: self.form_state.username.clone(),
                password,
                key_path,
                key_passphrase,
                last_connection_status: None,
            };

            self.connections[idx] = connection;
            Ok(())
        } else {
            Err("No connection selected")
        }
    }

    pub fn delete_connection(&mut self) {
        if let Some(idx) = self.selected_connection {
            self.connections.remove(idx);
            if idx >= self.connections.len() && idx > 0 {
                self.selected_connection = Some(idx - 1);
            }
        }
    }

    pub fn select_ssh_key(&mut self, direction: i8) {
        if self.form_state.active_field == 5 && !self.ssh_keys.is_empty() {
            let total_keys = self.ssh_keys.len();
            let current = self.form_state.selected_key.unwrap_or(0);
            
            let next_idx = if direction > 0 {
                (current + 1) % total_keys
            } else {
                if current == 0 {
                    total_keys - 1
                } else {
                    current - 1
                }
            };
            
            self.form_state.selected_key = Some(next_idx);
        }
    }

    pub fn connect_to_selected(&self) -> Result<(), AppError> {
        let idx = self.selected_connection.ok_or(AppError::NoConnectionSelected)?;
        let conn = &self.connections[idx];
        
        let tcp = TcpStream::connect(&format!("{}:{}", conn.host, conn.port))
            .map_err(|e| AppError::ConnectionFailed(e.to_string()))?;
        
        let mut sess = Session::new()
            .map_err(|e| AppError::ConnectionFailed(e.to_string()))?;
        sess.set_tcp_stream(tcp);
        sess.handshake()
            .map_err(|e| AppError::ConnectionFailed(e.to_string()))?;

        if let Some(key_path) = &conn.key_path {
            sess.userauth_pubkey_file(
                &conn.username,
                None,
                key_path,
                conn.key_passphrase.as_deref(),
            ).map_err(|e| AppError::AuthenticationFailed(e.to_string()))?;
        } else if let Some(password) = &conn.password {
            sess.userauth_password(&conn.username, password)
                .map_err(|e| AppError::AuthenticationFailed(e.to_string()))?;
        } else {
            return Err(AppError::AuthenticationFailed(
                "No authentication method provided".to_string()
            ));
        }

        let mut channel = sess.channel_session()
            .map_err(|e| AppError::ConnectionFailed(e.to_string()))?;
        channel.shell()
            .map_err(|e| AppError::ConnectionFailed(e.to_string()))?;
        channel.request_pty("xterm", None, None)
            .map_err(|e| AppError::ConnectionFailed(e.to_string()))?;

        Ok(())
    }

    pub fn add_key_path(&mut self, path: PathBuf) {
        if path.exists() && path.is_file() {
            if !self.ssh_keys.contains(&path) {
                self.additional_key_paths.push(path.clone());
                self.ssh_keys.push(path);
            }
        }
    }

    pub fn show_error<T: Into<String>>(&mut self, message: T) {
        self.error_message = Some(message.into());
    }

    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    pub fn select_key_file(&mut self) -> Result<()> {
        self.file_browser = Some(FileBrowser::new(dirs::home_dir().unwrap_or_default()));
        self.input_mode = InputMode::FileBrowser(FileBrowserMode::SingleFile);
        Ok(())
    }

    pub fn select_key_folder(&mut self) -> Result<()> {
        self.file_browser = Some(FileBrowser::new(dirs::home_dir().unwrap_or_default()));
        self.input_mode = InputMode::FileBrowser(FileBrowserMode::Directory);
        Ok(())
    }

    pub fn test_connection(&mut self, idx: usize) -> Result<(), AppError> {
        if idx >= self.connections.len() {
            return Err(AppError::NoConnectionSelected);
        }
        
        let conn = &mut self.connections[idx];
        
        let result = (|| {
            let tcp = TcpStream::connect(format!("{}:{}", conn.host, conn.port))
                .map_err(|e| AppError::ConnectionFailed(e.to_string()))?;
            
            let mut sess = Session::new()
                .map_err(|e| AppError::ConnectionFailed(e.to_string()))?;
            sess.set_tcp_stream(tcp);
            sess.handshake()
                .map_err(|e| AppError::ConnectionFailed(e.to_string()))?;

            if let Some(key_path) = &conn.key_path {
                sess.userauth_pubkey_file(
                    &conn.username,
                    None,
                    key_path,
                    conn.key_passphrase.as_deref(),
                ).map_err(|e| AppError::AuthenticationFailed(e.to_string()))?;
            } else if let Some(password) = &conn.password {
                sess.userauth_password(&conn.username, password)
                    .map_err(|e| AppError::AuthenticationFailed(e.to_string()))?;
            } else {
                return Err(AppError::AuthenticationFailed(
                    "No authentication method provided".to_string()
                ));
            }
            Ok(())
        })();

        conn.last_connection_status = Some(result.is_ok());
        result
    }

    pub fn execute_ssh(&self) -> Result<bool, AppError> {
        let idx = self.selected_connection.ok_or(AppError::NoConnectionSelected)?;
        if idx >= self.connections.len() {
            return Err(AppError::NoConnectionSelected);
        }
        
        let conn = &self.connections[idx];
        
        let mut cmd;
        if let Some(password) = &conn.password {
            if conn.key_path.is_none() {
                cmd = Command::new("sshpass");
                cmd.arg("-p").arg(password);
                cmd.arg("ssh");
            } else {
                cmd = Command::new("ssh");
            }
        } else {
            cmd = Command::new("ssh");
        }
        
        if conn.port != 22 {
            cmd.arg("-p").arg(conn.port.to_string());
        }
        
        let mut connection_args = Vec::new();
        
        if let Some(key_path) = &conn.key_path {
            connection_args.push("-i".to_string());
            connection_args.push(key_path.to_string_lossy().to_string());
            
            if let Some(passphrase) = &conn.key_passphrase {
                let mut ssh_args = connection_args.clone();
                
                let conn_string = format!("{}@{}", conn.username, conn.host);
                ssh_args.push(conn_string);
                
                cmd = Command::new("sshpass");
                cmd.arg("-P").arg("Enter passphrase for key");
                cmd.arg("-p").arg(passphrase);
                
                cmd.arg("ssh");
                for arg in ssh_args {
                    cmd.arg(arg);
                }
                
                disable_raw_mode().map_err(|e| AppError::ConnectionFailed(format!("Failed to reset terminal mode: {}", e)))?;
                crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen, DisableMouseCapture)
                    .map_err(|e| AppError::ConnectionFailed(format!("Failed to leave alternate screen: {}", e)))?;
                std::io::stdout().flush().map_err(|e| AppError::ConnectionFailed(format!("Failed to flush stdout: {}", e)))?;

                cmd.env("TERM", "xterm-256color")
                    .stdin(std::process::Stdio::inherit())
                    .stdout(std::process::Stdio::inherit())
                    .stderr(std::process::Stdio::inherit());
                let status = cmd.status().map_err(|e| AppError::ConnectionFailed(format!("Failed to execute SSH: {}", e)))?;
                if !status.success() {
                    return Err(AppError::ConnectionFailed("SSH process failed".to_string()));
                }

                thread::sleep(Duration::from_millis(50));

                crossterm::execute!(
                    std::io::stdout(),
                    Clear(ClearType::All),
                    crossterm::terminal::EnterAlternateScreen,
                    EnableMouseCapture
                ).map_err(|e| AppError::ConnectionFailed(format!("Failed to restore terminal state: {}", e)))?;
                std::io::stdout().flush().map_err(|e| AppError::ConnectionFailed(format!("Failed to flush stdout: {}", e)))?;
                
                enable_raw_mode().map_err(|e| AppError::ConnectionFailed(format!("Failed to restore terminal mode: {}", e)))?;
                
                return Ok(true);
            }
        }
        
        for arg in connection_args {
            cmd.arg(arg);
        }
        
        let connection_string = format!("{}@{}", conn.username, conn.host);
        cmd.arg(connection_string);

        disable_raw_mode().map_err(|e| AppError::ConnectionFailed(format!("Failed to reset terminal mode: {}", e)))?;
        crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen, DisableMouseCapture)
            .map_err(|e| AppError::ConnectionFailed(format!("Failed to leave alternate screen: {}", e)))?;
        std::io::stdout().flush().map_err(|e| AppError::ConnectionFailed(format!("Failed to flush stdout: {}", e)))?;

        cmd.env("TERM", "xterm-256color")
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());
        let status = cmd.status().map_err(|e| AppError::ConnectionFailed(format!("Failed to execute SSH: {}", e)))?;
        if !status.success() {
            return Err(AppError::ConnectionFailed("SSH process failed".to_string()));
        }

        thread::sleep(Duration::from_millis(50));

        crossterm::execute!(
            std::io::stdout(),
            Clear(ClearType::All),
            crossterm::terminal::EnterAlternateScreen,
            EnableMouseCapture
        ).map_err(|e| AppError::ConnectionFailed(format!("Failed to restore terminal state: {}", e)))?;
        std::io::stdout().flush().map_err(|e| AppError::ConnectionFailed(format!("Failed to flush stdout: {}", e)))?;
        
        enable_raw_mode().map_err(|e| AppError::ConnectionFailed(format!("Failed to restore terminal mode: {}", e)))?;
        
        Ok(true)
    }

    pub fn save_additional_keys(&self) -> Result<()> {
        let config_dir = dirs::config_dir()
            .context("Could not find config directory")?
            .join("peroxide");
        
        fs::create_dir_all(&config_dir)?;
        let keys_file = config_dir.join("additional_keys.json");
        
        let content = serde_json::to_string_pretty(&self.additional_key_paths)?;
        fs::write(keys_file, content)?;
        Ok(())
    }

    pub fn load_additional_keys() -> Result<Vec<PathBuf>> {
        let config_dir = dirs::config_dir()
            .context("Could not find config directory")?
            .join("peroxide");
        
        let keys_file = config_dir.join("additional_keys.json");
        
        if !keys_file.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(keys_file)?;
        let paths = serde_json::from_str(&content)?;
        Ok(paths)
    }

    pub fn duplicate_connection(&mut self) -> Result<(), &'static str> {
        if self.connections.is_empty() {
            return Err("No connections to duplicate");
        }
        
        if let Some(idx) = self.selected_connection {
            if idx >= self.connections.len() {
                return Err("Invalid connection selected");
            }
            
            if let Some(conn) = self.connections.get(idx) {
                let mut new_conn = conn.clone();
                new_conn.name = format!("{} (copy)", conn.name);
                new_conn.last_connection_status = None;
                self.connections.push(new_conn);
                self.selected_connection = Some(self.connections.len() - 1);
                Ok(())
            } else {
                Err("Failed to get connection")
            }
        } else {
            Err("No connection selected")
        }
    }

    pub fn next_settings_tab(&mut self) {
    }

    pub fn remove_ssh_key(&mut self, index: usize) {
        if index < self.ssh_keys.len() {
            let path = self.ssh_keys[index].clone();
            self.ssh_keys.remove(index);
            
            if let Some(additional_index) = self.additional_key_paths.iter().position(|p| p == &path) {
                self.additional_key_paths.remove(additional_index);
            }
            
            if self.settings_selected_item > 3 && self.settings_selected_item >= 3 + self.ssh_keys.len() {
                self.settings_selected_item -= 1;
            }
        }
    }
} 