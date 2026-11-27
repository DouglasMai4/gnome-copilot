import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import GObject from 'gi://GObject';
import Pango from 'gi://Pango';
import Shell from 'gi://Shell';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {Spinner} from 'resource:///org/gnome/shell/ui/animation.js';

import {errorMessage} from './dbus.js';

const MAX_TURNS = 4;
const SPINNER_DELAY = 150;

const LONG_TEXT = 90;

const ACTION_ICONS = {
    open_app: 'application-x-executable-symbolic',
    open_settings: 'org.gnome.Settings-symbolic',
    open_folder: 'folder-symbolic',
    open_url: 'web-browser-symbolic',
    set_volume: 'audio-volume-high-symbolic',
    set_mute: 'audio-volume-muted-symbolic',
    set_dark_mode: 'dark-mode-symbolic',
    set_night_light: 'night-light-symbolic',
    set_do_not_disturb: 'notifications-disabled-symbolic',
    take_screenshot: 'camera-photo-symbolic',
    close_app: 'window-close-symbolic',
    lock_screen: 'system-lock-screen-symbolic',
    power: 'system-shutdown-symbolic',
    run_command: 'utilities-terminal-symbolic',
};

const SUGGESTIONS = [
    {label: 'Abrir o terminal', query: 'abrir o terminal', icon: 'utilities-terminal-symbolic'},
    {label: 'Estilo escuro', query: 'ativar o estilo escuro', icon: 'dark-mode-symbolic'},
    {label: 'Volume em 50%', query: 'volume em 50%', icon: 'audio-volume-high-symbolic'},
    {label: 'Tirar um print', query: 'tirar um print', icon: 'camera-photo-symbolic'},
    {label: 'Wi-Fi', query: 'abrir as configurações de wi-fi', icon: 'network-wireless-symbolic'},
    {label: 'Downloads', query: 'abrir a pasta downloads', icon: 'folder-download-symbolic'},
];

function normalize(s) {
    return s.normalize('NFD').replace(/\p{Diacritic}/gu, '').toLowerCase().trim();
}

function label(text, styleClass, wrap = false) {
    const l = new St.Label({text, style_class: styleClass, x_expand: true});
    if (wrap) {
        l.clutter_text.line_wrap = true;
        l.clutter_text.line_wrap_mode = Pango.WrapMode.WORD_CHAR;
        l.clutter_text.ellipsize = Pango.EllipsizeMode.NONE;
    }
    return l;
}

export const CopilotMenu = class extends PopupMenu.PopupMenu {
    constructor(sourceActor, side) {
        super(sourceActor, 0.5, side);
        this.box.add_style_class_name('quick-settings');
        this.box.add_style_class_name('copilot-menu');
        this.actor.add_style_class_name('copilot-menu-boxpointer');
        Main.layoutManager.connectObject('system-modal-opened', () => this.close(), this);
    }

    isEmpty() {
        return false;
    }
};

export const CopilotPanel = GObject.registerClass(
class CopilotPanel extends St.BoxLayout {
    constructor(client, {openSettings, closeMenu, extensionPath}) {
        super({
            style_class: 'copilot-panel',
            orientation: Clutter.Orientation.VERTICAL,
            x_expand: true,
        });
        this._client = client;
        this._openSettings = openSettings;
        this._closeMenu = closeMenu;
        this._icon = Gio.FileIcon.new(Gio.File.new_for_path(`${extensionPath}/icons/copilot-symbolic.svg`));
        this._status = 'unknown';
        this._busy = false;
        this._spinnerTimeout = 0;
        this._downloadError = '';
        this._progress = 0;
        this._pending = new Set();

        this._buildHeader();
        this._buildBody();
        this._buildEntry();

        this._client.on('StatusChanged', status => this._setStatus(status));
        this._client.on('DownloadProgress', (id, fraction) => this._onProgress(id, fraction));
        this._client.on('DownloadFinished', (_id, ok, error) => {
            this._downloadError = ok ? '' : error;
            this.refresh();
        });
        this.refresh();
    }

    _buildHeader() {
        const header = new St.BoxLayout({style_class: 'copilot-header', x_expand: true});
        this.add_child(header);

        this._modelButton = new St.Button({
            style_class: 'icon-button copilot-model-pill',
            can_focus: true,
            accessible_name: 'Modelo ativo',
        });
        const pillBox = new St.BoxLayout({style_class: 'copilot-model-pill-box'});
        this._modelIcon = new St.Icon({gicon: this._icon, style_class: 'copilot-model-icon'});
        this._modelLabel = new St.Label({text: 'Copilot', y_align: Clutter.ActorAlign.CENTER});
        pillBox.add_child(this._modelIcon);
        pillBox.add_child(this._modelLabel);
        this._modelButton.set_child(pillBox);
        this._modelButton.connect('clicked', () => this._openSettings('models'));
        header.add_child(this._modelButton);

        header.add_child(new St.Widget({x_expand: true}));

        this._clearButton = new St.Button({
            style_class: 'icon-button',
            child: new St.Icon({icon_name: 'edit-clear-all-symbolic'}),
            can_focus: true,
            visible: false,
            accessible_name: 'Limpar histórico',
        });
        this._clearButton.connect('clicked', () => this.clearHistory());
        header.add_child(this._clearButton);

        this._micButton = new St.Button({
            style_class: 'icon-button',
            child: new St.Icon({icon_name: 'audio-input-microphone-symbolic'}),
            reactive: false,
            can_focus: false,
            accessible_name: 'Falar (em breve)',
        });
        this._micButton.add_style_pseudo_class('insensitive');
        header.add_child(this._micButton);

        const settings = new St.Button({
            style_class: 'icon-button',
            child: new St.Icon({icon_name: 'org.gnome.Settings-symbolic'}),
            can_focus: true,
            accessible_name: 'Configurações do Copilot',
        });
        settings.connect('clicked', () => this._openSettings());
        header.add_child(settings);
    }

    _buildEntry() {
        this._entry = new St.Entry({
            style_class: 'copilot-entry',
            hint_text: 'Peça algo ao Copilot…',
            can_focus: true,
            x_expand: true,
        });
        this._entry.clutter_text.connect('activate', () => this._submit(this._entry.text));
        this._entry.clutter_text.connect('text-changed', () => this._updateSendIcon());

        this._sendIcon = new St.Button({
            style_class: 'copilot-send',
            child: new St.Icon({icon_name: 'go-up-symbolic'}),
            can_focus: true,
            accessible_name: 'Enviar',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._sendIcon.connect('clicked', () => this._submit(this._entry.text));
        this._spinner = new Spinner(16, {animate: true, hideOnStop: true});
        this.add_child(this._entry);
        this._updateSendIcon();
    }

    _buildBody() {
        this._scroll = new St.ScrollView({
            style_class: 'copilot-scroll',
            hscrollbar_policy: St.PolicyType.NEVER,
            vscrollbar_policy: St.PolicyType.AUTOMATIC,
            overlay_scrollbars: true,
            x_expand: true,
            y_expand: true,
        });
        this._body = new St.BoxLayout({
            style_class: 'copilot-body',
            orientation: Clutter.Orientation.VERTICAL,
            x_expand: true,
        });
        this._scroll.set_child(this._body);

        this._stickToBottom = true;
        const adj = this._scroll.vadjustment;
        adj.connect('notify::value', () => {
            this._stickToBottom = adj.value >= adj.upper - adj.page_size - 8;
        });
        adj.connect('notify::upper', () => {
            if (this._stickToBottom)
                adj.value = adj.upper - adj.page_size;
        });
        this.add_child(this._scroll);

        this._history = new St.BoxLayout({
            style_class: 'copilot-history',
            orientation: Clutter.Orientation.VERTICAL,
            x_expand: true,
        });
        this._suggestions = this._buildSuggestions();
        this._setup = new St.BoxLayout({
            style_class: 'copilot-setup',
            orientation: Clutter.Orientation.VERTICAL,
            x_expand: true,
        });
        this._body.add_child(this._setup);
        this._body.add_child(this._history);
        this._body.add_child(this._suggestions);
    }

    _buildSuggestions() {
        const layout = new Clutter.GridLayout({orientation: Clutter.Orientation.HORIZONTAL});
        const grid = new St.Widget({style_class: 'quick-settings-grid copilot-suggestions', layout_manager: layout});
        layout.hookup_style(grid);
        SUGGESTIONS.forEach((s, i) => {
            const button = new St.Button({
                style_class: 'quick-toggle button copilot-suggestion',
                can_focus: true,
                x_expand: true,
                accessible_name: s.label,
            });
            const box = new St.BoxLayout({x_expand: true});
            box.add_child(new St.Icon({icon_name: s.icon, style_class: 'quick-toggle-icon'}));
            const l = new St.Label({
                text: s.label,
                style_class: 'quick-toggle-title',
                y_align: Clutter.ActorAlign.CENTER,
                x_expand: true,
            });
            l.clutter_text.ellipsize = Pango.EllipsizeMode.END;
            box.add_child(l);
            button.set_child(box);
            button.connect('clicked', () => this._submit(s.query));
            layout.attach(button, i % 2, Math.floor(i / 2), 1, 1);
        });
        return grid;
    }

    async refresh() {
        try {
            this._setStatus(await this._client.status());
        } catch (e) {
            this._setStatus('offline', errorMessage(e));
        }
    }

    _setStatus(status, detail = '') {
        this._status = status;
        const ready = status === 'ready';
        this._entry.visible = ready;
        this._entry.reactive = ready;
        this._entry.can_focus = ready;
        if (ready)
            this._entry.remove_style_pseudo_class('insensitive');
        else
            this._entry.add_style_pseudo_class('insensitive');

        this._modelLabel.text = {
            ready: 'Needle 3',
            downloading: `Baixando ${Math.round(this._progress * 100)}%`,
            no_model: 'Sem modelo',
            offline: 'Serviço parado',
        }[status] ?? 'Copilot';
        if (ready)
            this._modelLabel.text = this._activeModelName ?? 'Needle 3';

        this._renderSetup(status, detail);
        this._suggestions.visible = ready && this._history.get_n_children() === 0;
        if (ready)
            this._loadModelName();
    }

    async _loadModelName() {
        try {
            const active = (await this._client.models()).find(m => m.active);
            if (active) {
                this._activeModelName = active.name;
                if (this._status === 'ready')
                    this._modelLabel.text = active.name;
            }
        } catch {
        }
    }

    _onProgress(_id, fraction) {
        this._progress = fraction;
        if (this._status !== 'downloading') {
            this._setStatus('downloading');
            return;
        }
        this._modelLabel.text = `Baixando ${Math.round(fraction * 100)}%`;
        this._progressFill?.set_width(Math.round((this._progressTrack?.width ?? 0) * fraction));
        if (this._progressLabel)
            this._progressLabel.text = `${Math.round(fraction * 35)} de 35 MB`;
    }

    _renderSetup(status, detail) {
        this._setup.destroy_all_children();
        this._progressTrack = this._progressFill = this._progressLabel = null;
        this._setup.visible = status !== 'ready';
        if (status === 'ready')
            return;

        const icon = new St.Bin({
            style_class: 'copilot-setup-icon',
            x_align: Clutter.ActorAlign.START,
            child: new St.Icon({
                icon_name: status === 'offline' ? 'dialog-warning-symbolic' : 'folder-download-symbolic',
            }),
        });
        this._setup.add_child(icon);

        if (status === 'offline') {
            this._setup.add_child(label('O serviço do Copilot não respondeu', 'copilot-setup-title', true));
            this._setup.add_child(label(
                detail || 'Verifique se o copilotd está instalado e tente de novo.',
                'copilot-setup-body', true));
            this._setup.add_child(this._primaryButton('Tentar de novo', () => this.refresh()));
            return;
        }

        if (status === 'downloading') {
            this._setup.add_child(label('Baixando o Needle 3…', 'copilot-setup-title', true));
            this._setup.add_child(label(
                'O modelo fica guardado só no seu computador. Pode fechar este painel; o download continua.',
                'copilot-setup-body', true));
            this._progressTrack = new St.Widget({style_class: 'copilot-progress', x_expand: true});
            this._progressFill = new St.Widget({style_class: 'copilot-progress-fill', width: 0});
            this._progressTrack.add_child(this._progressFill);
            this._progressTrack.connect('notify::width', () =>
                this._progressFill.set_width(Math.round(this._progressTrack.width * this._progress)));
            this._setup.add_child(this._progressTrack);
            this._progressLabel = label(`${Math.round(this._progress * 35)} de 35 MB`, 'copilot-setup-caption');
            this._setup.add_child(this._progressLabel);
            return;
        }

        this._setup.add_child(label('Escolha um modelo para começar', 'copilot-setup-title', true));
        this._setup.add_child(label(
            'O Copilot roda inteiramente no seu computador. O Needle 3 entende pedidos e executa ações; são 35 MB.',
            'copilot-setup-body', true));
        if (this._downloadError) {
            this._setup.add_child(label(`O download falhou: ${this._downloadError}`,
                'copilot-setup-body copilot-error', true));
        }
        const row = new St.BoxLayout({style_class: 'copilot-setup-actions', x_expand: true});
        row.add_child(this._primaryButton(this._downloadError ? 'Tentar de novo' : 'Baixar Needle 3', async () => {
            this._downloadError = '';
            this._progress = 0;
            try {
                await this._client.download('needle3');
            } catch (e) {
                this._downloadError = errorMessage(e);
                this.refresh();
            }
        }));
        const other = new St.Button({
            style_class: 'notification-button copilot-secondary',
            label: 'Ver modelos',
            can_focus: true,
        });
        other.connect('clicked', () => this._openSettings('models'));
        row.add_child(other);
        this._setup.add_child(row);
    }

    _primaryButton(text, callback) {
        const b = new St.Button({
            style_class: 'notification-button copilot-primary',
            label: text,
            can_focus: true,
        });
        b.connect('clicked', callback);
        return b;
    }

    _updateSendIcon() {
        if (this._busy) {
            if (this._entry.get_secondary_icon() !== this._spinner)
                this._entry.set_secondary_icon(this._spinner);
            this._spinner.play();
            return;
        }
        this._spinner.stop();
        if (this._entry.get_secondary_icon() !== this._sendIcon)
            this._entry.set_secondary_icon(this._sendIcon);
        const empty = !this._entry.text.trim();
        this._sendIcon.reactive = !empty;
        if (empty)
            this._sendIcon.add_style_pseudo_class('insensitive');
        else
            this._sendIcon.remove_style_pseudo_class('insensitive');
    }

    async _submit(text) {
        text = text.trim();
        if (!text || this._status !== 'ready')
            return;
        if (this._busy) {
            this._queued = text;
            return;
        }
        this._busy = true;
        this._spinnerTimeout = GLib.timeout_add(GLib.PRIORITY_DEFAULT, SPINNER_DELAY, () => {
            this._spinnerTimeout = 0;
            this._updateSendIcon();
            return GLib.SOURCE_REMOVE;
        });
        let reply;
        try {
            reply = await this._client.ask(text);
            if (this._entry.text.trim() === text)
                this._entry.text = '';
        } catch (e) {
            reply = {id: '', items: [], message: errorMessage(e), error: true};
        } finally {
            this._busy = false;
            if (this._spinnerTimeout) {
                GLib.source_remove(this._spinnerTimeout);
                this._spinnerTimeout = 0;
            }
            this._updateSendIcon();
        }
        this._addTurn(text, reply);
        if (this._queued) {
            const next = this._queued;
            this._queued = null;
            this._submit(next);
        }
    }

    _addTurn(text, reply) {
        const turn = new St.BoxLayout({
            style_class: 'copilot-turn',
            orientation: Clutter.Orientation.VERTICAL,
            x_expand: true,
            reactive: true,
            track_hover: true,
        });
        const bubble = new St.Bin({
            style_class: 'copilot-request',
            x_align: Clutter.ActorAlign.END,
            child: label(text, 'copilot-request-text', true),
        });
        turn.add_child(bubble);

        if (reply.message && reply.items.length === 0) {
            turn.add_child(this._card({
                icon: reply.error ? 'dialog-warning-symbolic' : 'dialog-question-symbolic',
                title: reply.error ? 'Algo deu errado' : 'Não entendi',
                subtitle: reply.message,
                tone: reply.error ? 'error' : 'neutral',
            }));
        }
        for (const item of reply.items)
            turn.add_child(this._itemCard(reply.id, item));
        if (reply.stats)
            this._addDetails(turn, reply.stats);

        this._history.add_child(turn);
        while (this._history.get_n_children() > MAX_TURNS)
            this._history.get_first_child().destroy();
        this._suggestions.visible = false;
        this._clearButton.visible = true;

        if (!St.Settings.get().enable_animations) {
            turn.opacity = 255;
        } else {
            turn.opacity = 0;
            turn.translation_y = 8;
            turn.ease({
                opacity: 255,
                translation_y: 0,
                duration: 220,
                mode: Clutter.AnimationMode.EASE_OUT_EXPO,
            });
        }
        this._stickToBottom = true;
    }

    _itemIcon(item) {
        if (item.app_id) {
            const app = Shell.AppSystem.get_default().lookup_app(`${item.app_id}.desktop`);
            if (app)
                return app.create_icon_texture(28);
        }
        return ACTION_ICONS[item.action] ?? 'object-select-symbolic';
    }

    _itemCard(requestId, item) {
        switch (item.status) {
        case 'done': {
            const msg = item.message ?? '';
            const long = msg.length > LONG_TEXT || msg.includes('\n');
            return this._card({
                icon: this._itemIcon(item),
                title: long || !msg ? item.title : msg,
                subtitle: long ? msg : '',
                tone: 'done',
            });
        }
        case 'failed':
            return this._card({
                icon: this._itemIcon(item),
                title: item.title,
                subtitle: item.message || 'Não foi possível concluir',
                tone: 'error',
            });
        case 'shell':
            return this._runShellAction(item);
        case 'needs_confirmation':
        case 'suggestion': {
            const suggestion = item.status === 'suggestion';
            this._pending.add(requestId);
            const card = this._card({
                icon: this._itemIcon(item),
                title: suggestion ? `Você quis dizer: ${item.title}?` : item.title,
                subtitle: suggestion ? '' : 'Confirme para continuar',
                tone: 'pending',
            });
            const actions = new St.BoxLayout({style_class: 'copilot-card-actions', x_align: Clutter.ActorAlign.END});
            const cancel = new St.Button({style_class: 'notification-button', label: 'Cancelar', can_focus: true});
            const run = new St.Button({
                style_class: 'notification-button copilot-primary',
                label: suggestion ? 'Sim, executar' : 'Executar',
                can_focus: true,
            });
            cancel.connect('clicked', () => {
                this._client.dismiss(requestId);
                card.replaceWith({icon: this._itemIcon(item), title: item.title, subtitle: 'Cancelado', tone: 'neutral'});
            });
            run.connect('clicked', async () => {
                run.reactive = cancel.reactive = false;
                try {
                    const result = await this._client.confirm(requestId, item.index);
                    if (result.status === 'shell') {
                        const shellCard = this._runShellAction(result);
                        card.replaceWithActor(shellCard);
                    } else {
                        const done = result.status === 'done';
                        card.replaceWith({
                            icon: this._itemIcon(result),
                            title: done ? result.message || result.title : result.title,
                            subtitle: done ? '' : result.message,
                            tone: done ? 'done' : 'error',
                        });
                    }
                } catch (e) {
                    card.replaceWith({icon: 'dialog-warning-symbolic', title: item.title, subtitle: errorMessage(e), tone: 'error'});
                }
            });
            actions.add_child(cancel);
            actions.add_child(run);
            card.textBox.add_child(actions);
            return card;
        }
        default:
            return this._card({icon: 'dialog-question-symbolic', title: item.title, subtitle: item.message, tone: 'neutral'});
        }
    }

    _card({icon, title, subtitle, tone}) {
        const card = new St.BoxLayout({style_class: `copilot-card copilot-card-${tone}`, x_expand: true});
        const iconBin = new St.Bin({style_class: 'copilot-card-icon', y_align: Clutter.ActorAlign.START});
        iconBin.set_child(typeof icon === 'string' ? new St.Icon({icon_name: icon}) : icon);
        if (typeof icon !== 'string')
            iconBin.add_style_class_name('copilot-card-app-icon');
        card.add_child(iconBin);

        const textBox = new St.BoxLayout({
            style_class: 'copilot-card-text',
            orientation: Clutter.Orientation.VERTICAL,
            x_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
        });
        textBox.add_child(label(title, 'copilot-card-title', true));
        const sub = subtitle ? label(subtitle, 'copilot-card-subtitle', true) : null;
        if (sub)
            textBox.add_child(sub);
        card.add_child(textBox);
        card.textBox = textBox;

        const long = subtitle && (subtitle.length > LONG_TEXT || subtitle.includes('\n'));
        if (long) {
            const chevron = new St.Icon({icon_name: 'pan-down-symbolic'});
            const toggle = new St.Button({
                style_class: 'icon-button copilot-expand',
                child: chevron,
                can_focus: true,
                y_align: Clutter.ActorAlign.START,
                accessible_name: 'Mostrar tudo',
            });
            const setExpanded = expanded => {
                sub.clutter_text.line_wrap = expanded;
                sub.clutter_text.ellipsize = expanded ? Pango.EllipsizeMode.NONE : Pango.EllipsizeMode.END;
                sub.text = expanded ? subtitle : subtitle.split('\n')[0];
                chevron.icon_name = expanded ? 'pan-up-symbolic' : 'pan-down-symbolic';
                toggle.accessible_name = expanded ? 'Mostrar menos' : 'Mostrar tudo';
                toggle.expanded = expanded;
            };
            setExpanded(false);
            toggle.connect('clicked', () => setExpanded(!toggle.expanded));
            card.add_child(toggle);
        }

        card.replaceWith = spec => card.replaceWithActor(this._card(spec));
        card.replaceWithActor = actor => {
            const parent = card.get_parent();
            if (!parent)
                return;
            parent.insert_child_above(actor, card);
            card.destroy();
        };
        return card;
    }

    _addDetails(turn, stats) {
        const num = (v, digits = 0) => new Intl.NumberFormat('pt-BR', {maximumFractionDigits: digits}).format(v);
        const seconds = ms => `${num(ms / 1000, 2)} s`;

        const meta = new St.Button({
            style_class: 'copilot-meta',
            label: seconds(stats.total_ms),
            can_focus: true,
            x_align: Clutter.ActorAlign.START,
            accessible_name: 'Detalhes do processamento',
            opacity: 0,
        });
        const details = new St.BoxLayout({
            style_class: 'copilot-details',
            orientation: Clutter.Orientation.VERTICAL,
            visible: false,
            x_expand: true,
        });
        const rows = [
            ['Tempo total', seconds(stats.total_ms)],
            ['Modelo pensando', seconds(stats.inference_ms)],
            ['Confiança', stats.confidence != null ? `${num(stats.confidence * 100)}%` : null],
            ['Leitura do pedido', stats.prefill_tps ? `${num(stats.prefill_tps)} tokens/s` : null],
            ['Geração', stats.decode_tps ? `${num(stats.decode_tps)} tokens/s` : null],
            ['Memória (pico)', stats.peak_ram_mb ? `${num(stats.peak_ram_mb)} MB` : null],
            ['Modelo', stats.model],
        ].filter(([, v]) => v);
        for (const [k, v] of rows) {
            const row = new St.BoxLayout({style_class: 'copilot-details-row', x_expand: true});
            row.add_child(new St.Label({text: k, style_class: 'copilot-details-key', x_expand: true}));
            row.add_child(new St.Label({text: v, style_class: 'copilot-details-value'}));
            details.add_child(row);
        }
        if (stats.reasoning)
            details.add_child(label(stats.reasoning, 'copilot-details-reasoning', true));

        const reveal = () => {
            const show = turn.hover || meta.has_key_focus() || details.visible;
            meta.ease({opacity: show ? 255 : 0, duration: 150, mode: Clutter.AnimationMode.EASE_OUT_QUAD});
        };
        turn.connect('notify::hover', reveal);
        meta.connect('key-focus-in', reveal);
        meta.connect('key-focus-out', reveal);
        meta.connect('clicked', () => {
            details.visible = !details.visible;
            if (details.visible)
                meta.add_style_pseudo_class('checked');
            else
                meta.remove_style_pseudo_class('checked');
            reveal();
        });
        turn.add_child(meta);
        turn.add_child(details);
    }

    _runShellAction(item) {
        try {
            const message = this._shellAction(item);
            this._client.shellResult(item.action, true, message);
            return this._card({icon: this._itemIcon(item), title: message, tone: 'done'});
        } catch (e) {
            this._client.shellResult(item.action, false, e.message);
            return this._card({icon: this._itemIcon(item), title: item.title, subtitle: e.message, tone: 'error'});
        }
    }

    _shellAction(item) {
        switch (item.action) {
        case 'take_screenshot':

            this._closeMenu();
            GLib.timeout_add(GLib.PRIORITY_DEFAULT, 250, () => {
                Main.screenshotUI.open().catch(e => logError(e, 'GNOME Copilot: screenshot'));
                return GLib.SOURCE_REMOVE;
            });
            return 'Ferramenta de captura aberta';
        case 'close_app': {
            const query = normalize(String(item.arguments?.name ?? ''));
            const running = Shell.AppSystem.get_default().get_running();
            const app = running.find(a => item.app_id && a.get_id() === `${item.app_id}.desktop`) ??
                running.find(a => normalize(a.get_name()) === query) ??
                running.find(a => normalize(a.get_name()).includes(query) || normalize(a.get_id()).includes(query));
            if (!app)
                throw new Error(`${item.arguments?.name ?? 'Esse app'} não está aberto`);
            app.request_quit();
            return `${app.get_name()} foi fechado`;
        }
        default:
            throw new Error('Ação desconhecida');
        }
    }

    clearHistory() {
        for (const id of this._pending)
            this._client.dismiss(id);
        this._pending.clear();
        this._history.destroy_all_children();
        this._clearButton.visible = false;
        this._suggestions.visible = this._status === 'ready';
        if (this._status === 'ready')
            this._entry.grab_key_focus();
    }

    onOpened() {
        this.refresh();
        GLib.idle_add(GLib.PRIORITY_DEFAULT, () => {
            if (this._status === 'ready')
                this._entry.grab_key_focus();
            return GLib.SOURCE_REMOVE;
        });
    }

    destroy() {
        if (this._spinnerTimeout)
            GLib.source_remove(this._spinnerTimeout);
        for (const id of this._pending)
            this._client.dismiss(id);
        super.destroy();
    }
});
