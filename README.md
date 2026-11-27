# GNOME Copilot

A local, private AI assistant for GNOME. Open it from a notch on the screen edge or from the top bar, type a request ("open Firefox", "turn on dark style", "Wi-Fi settings") and it performs the action on your system.

Everything runs on your machine, using [Cactus Compute](https://cactuscompute.com/) models. No model ships with the app: you pick one and download it from inside the tool.

## Features

- Edge notch you can drag along either side of the screen, a top bar icon, or both
- Floating chat panel styled like GNOME Quick Settings; it closes when you click outside
- System actions with **Needle 3** (~35 MB): launch apps, open Settings pages and folders, open websites, volume, dark style, night light, Do Not Disturb, screenshots, close apps, lock, power off
- Sensitive actions (closing apps, lock, power, shell commands) ask for confirmation first
- Per-request details on hover: timing, confidence, tokens/s, memory and the model's reasoning
- Preferences app (GTK4 + libadwaita) to download, switch and remove models and choose which actions are allowed

Voice (Whistle) and screen understanding (Mirage) are planned.

## Requirements

- GNOME Shell 47 or newer, on x86_64 Linux with glibc
- Rust 1.85+ (edition 2024), e.g. via [rustup](https://rustup.rs)
- [`just`](https://github.com/casey/just) and Python 3
- GTK4, libadwaita and GLib development files

On Fedora:

```sh
sudo dnf install gtk4-devel libadwaita-devel glib2-devel just
```

## Install

Everything is installed per user into `~/.local`; no root needed.

```sh
just install
```

The first time, log out and back in so GNOME Shell sees the new extension, then:

```sh
just enable
```

## First run

1. Click the notch on the right edge of the screen (or press `Super+C`).
2. Click **Baixar Needle 3**. The engine (~1 MB) and the model (~35 MB) are downloaded from Hugging Face and verified with SHA-256.
3. Type a request and press Enter.

Open the preferences from the gear icon in the panel, or run `copilot-settings`.

## Usage

| Request (PT or EN) | What happens |
| --- | --- |
| `abre o firefox` / `open firefox` | Launches the app |
| `configurações de wi-fi` | Opens Settings › Wi-Fi |
| `volume em 30%` | Sets the output volume |
| `ativa o modo escuro` | Switches to dark style |
| `tira um print` | Opens the screenshot tool |
| `bloqueia a tela` | Asks for confirmation, then locks |

## How it works

| Component | Path | Role |
| --- | --- | --- |
| `copilotd` | `crates/copilotd` | Daemon on the session bus (`io.github.GnomeCopilot`), started on demand. Downloads models, runs Needle, executes actions |
| `cactus` | `crates/cactus` | Safe bindings to `libcactus.so`, loaded at runtime |
| `copilot-core` | `crates/copilot-core` | Model catalog (pinned revisions and checksums), config, paths |
| `copilot-settings` | `crates/copilot-settings` | Preferences app |
| Shell extension | `extension/` | Notch, top bar icon and chat panel |

Data locations:

- Models and engine: `~/.local/share/gnome-copilot`
- Agent config: `~/.config/gnome-copilot/config.toml`
- Panel settings (trigger mode, notch side, shortcut): GSettings `org.gnome.shell.extensions.gnome-copilot`

Your requests never leave the machine. The network is only used to download models.

## Development

```sh
just              # list recipes
just build        # release build
just test         # unit tests
just eval         # intent accuracy with the installed model
just dev-shell    # nested GNOME Shell, isolated from your session (needs mutter-devkit)
just shot tools/testing/scenarios/chat.txt   # headless screenshots of the panel
```

`dev-shell` and `shot` use their own D-Bus session, dconf and extension directory, so they never change your real session.

GNOME Shell only reloads extension code on a new login. After changing the extension, run `just install`, then log out and back in, or use `just dev-shell`.

The stylesheets are generated: edit `tools/build-styles.py`, not the `.css` files.

## Uninstall

```sh
just uninstall
```

Downloaded models stay in `~/.local/share/gnome-copilot`; delete that folder to remove them.

## License

[MIT](LICENSE).

The Cactus engine and the Needle, Whistle and Mirage models are licensed under Apache-2.0 by [Cactus Compute](https://github.com/cactus-compute/cactus). They are not bundled: GNOME Copilot downloads them from Hugging Face on first use.
