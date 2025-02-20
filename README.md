# Peroxide

A terminal-based SSH connection manager written in Rust, featuring an intuitive TUI interface for managing and connecting to your SSH servers.

![image](https://github.com/user-attachments/assets/8719615d-fbd7-4420-a953-2752fc1677ae)![image](https://github.com/user-attachments/assets/f575ef87-c0d7-4efe-bc77-e5b063d8b6dc)



## Features

- 🔑 Support for both password and SSH key authentication
- 📁 Automatic SSH key discovery from `.ssh` directory
- 💾 Persistent storage of connections and settings
- 🔄 Connection testing functionality
- 🔍 Easy navigation with keyboard shortcuts
- 📝 Edit, duplicate, and delete connections
- 🎨 Terminal UI with multiple views and tabs

## Installation

### Arch User Repository

#### Binary

[![binary](https://img.shields.io/aur/version/peroxide-ssh-manager-bin)](https://aur.archlinux.org/packages/peroxide-ssh-manager-bin)

#### Git

[![git](https://img.shields.io/aur/version/peroxide-ssh-manager-git)](https://aur.archlinux.org/packages/peroxide-ssh-manager-git)


### From Release

Download the latest release from the [Releases](https://github.com/0xgingi/peroxide/releases) page.

```bash
cd $HOME/Downloads
sudo cp peroxide /usr/local/bin/
```

### From Source
Make sure you have Rust installed ([rustup](https://rustup.rs/)), then:

```bash
git clone https://github.com/0xgingi/peroxide.git
cd peroxide
cargo install --release
sudo cp target/release/peroxide /usr/local/bin/
```

## Usage

Simply run `peroxide` in your terminal to launch the application.

### Key Bindings

- `q` - Quit
- `a` - Add new connection
- `e` - Edit selected connection
- `d` - Delete selected connection
- `c` - Connect to selected server
- `t` - Test selected connection
- `s` - Open settings
- `Tab` - Switch between fields
- `Enter` - Confirm/Submit
- `Esc` - Cancel/Back

## Configuration

Peroxide automatically stores its configuration in:
- Linux: `~/.config/peroxide/`
- macOS: `~/Library/Application Support/peroxide/`
- Windows: `%APPDATA%\peroxide\`

## Notes

- Windows and MacOS have not been tested
