use std::path::{Path, PathBuf};
use std::fs;

#[derive(Debug)]
pub struct FileBrowser {
    pub current_path: PathBuf,
    pub entries: Vec<PathBuf>,
    pub selected: usize,
}

impl FileBrowser {
    pub fn new(start_path: PathBuf) -> Self {
        let mut browser = Self {
            current_path: start_path,
            entries: Vec::new(),
            selected: 0,
        };
        browser.refresh_entries();
        browser
    }

    pub fn refresh_entries(&mut self) {
        let mut entries = Vec::new();
        
        entries.push(self.current_path.clone());
        
        if let Some(_parent) = self.current_path.parent() {
            entries.push(self.current_path.join(".."));
        }

        if let Ok(read_dir) = fs::read_dir(&self.current_path) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                if path.is_dir() || path.is_file() {
                    entries.push(path);
                }
            }
        }

        entries.sort_by(|a, b| {
            let a_is_special = a == &self.current_path || a.ends_with("..");
            let b_is_special = b == &self.current_path || b.ends_with("..");
            
            if a_is_special && !b_is_special {
                std::cmp::Ordering::Less
            } else if !a_is_special && b_is_special {
                std::cmp::Ordering::Greater
            } else if a.is_dir() && !b.is_dir() {
                std::cmp::Ordering::Less
            } else if !a.is_dir() && b.is_dir() {
                std::cmp::Ordering::Greater
            } else {
                a.file_name()
                    .unwrap_or_default()
                    .cmp(b.file_name().unwrap_or_default())
            }
        });

        self.entries = entries;
        self.selected = 0;
    }

    pub fn enter_directory(&mut self) -> bool {
        if self.selected < self.entries.len() {
            let selected_path = &self.entries[self.selected];
            
            if selected_path.ends_with("..") {
                if let Some(parent) = self.current_path.parent() {
                    self.current_path = parent.to_path_buf();
                    self.refresh_entries();
                    return true;
                }
            } else if selected_path.is_dir() {
                self.current_path = selected_path.clone();
                self.refresh_entries();
                return true;
            }
        }
        false
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected < self.entries.len().saturating_sub(1) {
            self.selected += 1;
        }
    }

    pub fn get_selected_path(&self) -> Option<PathBuf> {
        self.entries.get(self.selected).cloned()
    }

    pub fn is_valid_ssh_key(&self, path: &Path) -> bool {
        if !path.is_file() {
            return false;
        }

        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        
        !file_name.contains("known_hosts") &&
        !file_name.contains("authorized_keys") &&
        !file_name.contains("config") &&
        !file_name.ends_with(".pub")
    }

    pub fn get_display_name(&self, path: &Path) -> String {
        if path == &self.current_path {
            ".".to_string()
        } else if path.ends_with("..") {
            "..".to_string()
        } else {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        }
    }
} 