prefix   := env("PREFIX", home_directory() / ".local")
bindir   := prefix / "bin"
datadir  := prefix / "share"
uuid     := "gnome-copilot@gnome-copilot.github.io"
ext_src  := "extension" / uuid
ext_dest := datadir / "gnome-shell/extensions" / uuid

# List recipes
default:
    @just --list

# Build everything in release mode
build: styles
    cargo build --release

# Generate the extension stylesheets
styles:
    python3 -I tools/build-styles.py

# Compile the extension GSettings schema
schemas:
    glib-compile-schemas --strict {{ext_src}}/schemas

# Run unit tests
test:
    cargo test --workspace

# Measure intent accuracy with the installed model
eval:
    CACTUS_LIB={{datadir}}/gnome-copilot/engine/4.1.4/libcactus.so \
    NEEDLE_WEIGHTS="$(ls {{datadir}}/gnome-copilot/models/needle3/*/needle3.cact | head -1)" \
    cargo test -p copilotd eval -- --nocapture

# Install into ~/.local (no root needed)
install: build schemas
    install -Dm755 target/release/copilotd {{bindir}}/copilotd
    install -Dm755 target/release/copilot-settings {{bindir}}/copilot-settings
    mkdir -p {{datadir}}/dbus-1/services {{datadir}}/applications {{datadir}}/icons/hicolor/symbolic/apps {{ext_dest}}
    sed 's|@BINDIR@|{{bindir}}|' data/io.github.GnomeCopilot.service.in > {{datadir}}/dbus-1/services/io.github.GnomeCopilot.service
    sed 's|@BINDIR@|{{bindir}}|' data/io.github.GnomeCopilot.Settings.desktop.in > {{datadir}}/applications/io.github.GnomeCopilot.Settings.desktop
    install -m644 data/io.github.GnomeCopilot-symbolic.svg {{datadir}}/icons/hicolor/symbolic/apps/
    cp -rT {{ext_src}} {{ext_dest}}
    @echo
    @echo "Instalado. Na primeira vez, saia e entre de novo na sessão e rode: just enable"

# Enable the Shell extension
enable:
    gnome-extensions enable {{uuid}}

# Remove everything except downloaded models
uninstall:
    -gnome-extensions uninstall {{uuid}}
    rm -f {{bindir}}/copilotd {{bindir}}/copilot-settings
    rm -f {{datadir}}/dbus-1/services/io.github.GnomeCopilot.service
    rm -f {{datadir}}/applications/io.github.GnomeCopilot.Settings.desktop
    rm -f {{datadir}}/icons/hicolor/symbolic/apps/io.github.GnomeCopilot-symbolic.svg
    @echo "Os modelos baixados continuam em {{datadir}}/gnome-copilot."

# Run a nested, isolated GNOME Shell
dev-shell: build schemas
    tools/dev-shell.sh

# Take headless screenshots: just shot SCENARIO [OUTDIR]
shot script outdir="/tmp/copilot-shots": build schemas
    SCRIPT={{script}} tools/shot.sh {{outdir}}
