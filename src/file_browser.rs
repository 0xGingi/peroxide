use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
pub struct FileBrowser {
    pub current_path: PathBuf,
    pub entries: Vec<PathBuf>,
    pub selected: usize,
    /// Folder mode lists the current folder first so it can be chosen.
    pub pick_folder: bool,
}

impl FileBrowser {
    pub fn new(start_path: PathBuf, pick_folder: bool) -> Self {
        let mut browser = Self {
            current_path: start_path,
            entries: Vec::new(),
            selected: 0,
            pick_folder,
        };
        browser.refresh_entries();
        browser
    }

    pub fn refresh_entries(&mut self) {
        let mut entries: Vec<PathBuf> = fs::read_dir(&self.current_path)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() || path.is_file())
            .collect();
        entries.sort_by_key(|p| (!p.is_dir(), p.file_name().unwrap_or_default().to_owned()));

        if self.current_path.parent().is_some() {
            entries.insert(0, self.current_path.join(".."));
        }
        if self.pick_folder {
            entries.insert(0, self.current_path.clone());
        }

        self.entries = entries;
        self.selected = 0;
    }

    pub fn enter_directory(&mut self) {
        match self.entries.get(self.selected) {
            Some(path) if path.ends_with("..") => self.go_up(),
            Some(path) if path.is_dir() && *path != self.current_path => {
                self.current_path = path.clone();
                self.refresh_entries();
            }
            _ => {}
        }
    }

    pub fn go_up(&mut self) {
        if let Some(parent) = self.current_path.parent() {
            self.current_path = parent.to_path_buf();
            self.refresh_entries();
        }
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    pub fn get_selected_path(&self) -> Option<PathBuf> {
        self.entries.get(self.selected).cloned()
    }
}
