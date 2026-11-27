import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

import {CopilotClient} from './dbus.js';
import {Notch} from './notch.js';
import {CopilotMenu, CopilotPanel} from './panel.js';

const SETTINGS_APP_ID = 'io.github.GnomeCopilot.Settings.desktop';

export default class CopilotExtension extends Extension {
    enable() {
        this._settings = this.getSettings();
        this._client = new CopilotClient();
        this._panel = new CopilotPanel(this._client, {
            openSettings: page => this._openSettings(page),
            closeMenu: () => this._openMenu?.close(),
            extensionPath: this.path,
        });
        this._menus = [];
        this._openMenu = null;

        this._settings.connectObject(
            'changed::trigger-mode', () => this._rebuildTriggers(),
            'changed::notch-side', () => this._rebuildNotchMenu(),
            this);
        this._rebuildTriggers();

        Main.wm.addKeybinding('toggle-shortcut', this._settings,
            Meta.KeyBindingFlags.IGNORE_AUTOREPEAT,
            Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW,
            () => this._toggleFromShortcut());
    }

    disable() {
        Main.wm.removeKeybinding('toggle-shortcut');
        this._settings.disconnectObject(this);
        this._destroyTriggers();
        this._panel.destroy();
        this._panel = null;
        this._client.destroy();
        this._client = null;
        this._settings = null;
    }

    _rebuildTriggers() {
        this._destroyTriggers();
        const mode = this._settings.get_string('trigger-mode');
        if (mode === 'notch' || mode === 'both')
            this._createNotch();
        if (mode === 'tray' || mode === 'both')
            this._createIndicator();
    }

    _destroyTriggers() {
        this._openMenu?.close(false);
        this._detachPanel();
        this._destroyNotch();
        this._indicator?.destroy();
        this._indicator = null;
        this._menus = [];
        this._openMenu = null;
    }

    _createNotch() {
        this._notch = new Notch(this._settings);
        Main.layoutManager.addChrome(this._notch, {
            trackFullscreen: true,
        });
        this._notch.place(false);
        this._notchMenuManager = new PopupMenu.PopupMenuManager(this._notch);
        this._createNotchMenu();
        this._notch.connect('activate', () => this._notchMenu.toggle());
    }

    _createNotchMenu() {
        this._notchMenu = new CopilotMenu(this._notch, this._notch.side);
        Main.uiGroup.add_child(this._notchMenu.actor);
        this._notchMenu.actor.hide();
        this._notchMenuManager.addMenu(this._notchMenu);
        this._trackMenu(this._notchMenu);
    }

    _rebuildNotchMenu() {
        if (!this._notch)
            return;
        const wasOpen = this._notchMenu.isOpen;
        this._notchMenu.close(false);
        this._detachPanel();
        this._menus = this._menus.filter(m => m !== this._notchMenu);
        this._notchMenuManager.removeMenu(this._notchMenu);
        this._notchMenu.destroy();
        this._createNotchMenu();
        if (wasOpen)
            this._notchMenu.open();
    }

    _destroyNotch() {
        if (!this._notch)
            return;
        this._notchMenu?.destroy();
        this._notchMenu = null;
        this._notchMenuManager = null;
        this._notch.destroy();
        this._notch = null;
    }

    _createIndicator() {
        this._indicator = new PanelMenu.Button(0.5, 'Copilot', true);
        this._indicator.add_child(new St.Icon({
            gicon: Gio.FileIcon.new(Gio.File.new_for_path(`${this.path}/icons/copilot-symbolic.svg`)),
            style_class: 'system-status-icon',
        }));
        const menu = new CopilotMenu(this._indicator, St.Side.TOP);
        this._indicator.setMenu(menu);
        this._trackMenu(menu);
        Main.panel.addToStatusArea(this.uuid, this._indicator);
    }

    _trackMenu(menu) {
        this._menus.push(menu);
        menu.connect('open-state-changed', (m, open) => {
            if (open) {
                this._openMenu = m;
                this._detachPanel();
                m.box.add_child(this._panel);
                this._panel.onOpened();
            } else if (this._openMenu === m) {
                this._openMenu = null;
            }
        });
        menu.connect('destroy', () => {
            if (this._panel?.get_parent() === menu.box)
                menu.box.remove_child(this._panel);
        });
    }

    _detachPanel() {
        this._panel?.get_parent()?.remove_child(this._panel);
    }

    _toggleFromShortcut() {
        if (this._openMenu) {
            this._openMenu.close();
            return;
        }
        if (Main.overview.visible)
            Main.overview.hide();
        const menu = this._notchMenu ?? this._indicator?.menu;
        menu?.open();
    }

    _openSettings(page) {
        this._openMenu?.close();
        const exe = this._settingsExecutable();
        if (!exe) {
            Main.notifyError('Não foi possível abrir as configurações do Copilot',
                'O copilot-settings não está instalado. Rode “just install” na pasta do projeto.');
            return;
        }
        const argv = [exe];
        if (page)
            argv.push('--page', page);
        try {
            GLib.spawn_async(null, argv, null, GLib.SpawnFlags.DEFAULT, null);
        } catch (e) {
            Main.notifyError('Não foi possível abrir as configurações do Copilot', e.message);
        }
    }

    _settingsExecutable() {
        const info = Gio.DesktopAppInfo.new(SETTINGS_APP_ID);
        const fromDesktop = info?.get_executable();
        if (fromDesktop && GLib.path_is_absolute(fromDesktop) &&
            GLib.file_test(fromDesktop, GLib.FileTest.IS_EXECUTABLE))
            return fromDesktop;
        const fromPath = GLib.find_program_in_path('copilot-settings');
        if (fromPath)
            return fromPath;
        const local = GLib.build_filenamev([GLib.get_home_dir(), '.local', 'bin', 'copilot-settings']);
        return GLib.file_test(local, GLib.FileTest.IS_EXECUTABLE) ? local : null;
    }
}
