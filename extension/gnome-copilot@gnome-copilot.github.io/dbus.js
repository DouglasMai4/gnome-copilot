import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

const BUS_NAME = 'io.github.GnomeCopilot';
const OBJECT_PATH = '/io/github/GnomeCopilot';

const IFACE = `
<node>
  <interface name="io.github.GnomeCopilot1">
    <method name="GetStatus"><arg type="s" direction="out"/></method>
    <method name="Ask">
      <arg type="s" name="text" direction="in"/>
      <arg type="s" direction="out"/>
    </method>
    <method name="Confirm">
      <arg type="s" name="id" direction="in"/>
      <arg type="u" name="index" direction="in"/>
      <arg type="s" direction="out"/>
    </method>
    <method name="Dismiss"><arg type="s" name="id" direction="in"/></method>
    <method name="ShellResult">
      <arg type="s" name="action" direction="in"/>
      <arg type="b" name="ok" direction="in"/>
      <arg type="s" name="message" direction="in"/>
    </method>
    <method name="GetModels"><arg type="s" direction="out"/></method>
    <method name="Download"><arg type="s" name="id" direction="in"/></method>
    <signal name="StatusChanged"><arg type="s" name="status"/></signal>
    <signal name="ModelsChanged"/>
    <signal name="DownloadProgress">
      <arg type="s" name="id"/>
      <arg type="d" name="fraction"/>
    </signal>
    <signal name="DownloadFinished">
      <arg type="s" name="id"/>
      <arg type="b" name="ok"/>
      <arg type="s" name="error"/>
    </signal>
  </interface>
</node>`;

const Proxy = Gio.DBusProxy.makeProxyWrapper(IFACE);

export function errorMessage(e) {
    if (e instanceof GLib.Error)
        Gio.DBusError.strip_remote_error(e);
    return e?.message ?? String(e);
}

export class CopilotClient {
    constructor() {
        this._proxy = null;
        this._signals = [];
        this._listeners = new Map();
        this._ready = Proxy.newAsync(Gio.DBus.session, BUS_NAME, OBJECT_PATH, null,
            Gio.DBusProxyFlags.DO_NOT_LOAD_PROPERTIES).then(proxy => {
            this._proxy = proxy;
            for (const name of ['StatusChanged', 'ModelsChanged', 'DownloadProgress', 'DownloadFinished']) {
                this._signals.push(proxy.connectSignal(name,
                    (_p, _sender, args) => this._emit(name, ...args)));
            }
            return proxy;
        });
    }

    on(signal, callback) {
        if (!this._listeners.has(signal))
            this._listeners.set(signal, new Set());
        this._listeners.get(signal).add(callback);
        return () => this._listeners.get(signal)?.delete(callback);
    }

    _emit(signal, ...args) {
        for (const cb of this._listeners.get(signal) ?? []) {
            try {
                cb(...args);
            } catch (e) {
                logError(e, `GNOME Copilot: ${signal} handler`);
            }
        }
    }

    async _call(method, ...args) {
        const proxy = await this._ready;
        const [result] = await proxy[`${method}Async`](...args);
        return result;
    }

    async status() {
        return this._call('GetStatus');
    }

    async ask(text) {
        return JSON.parse(await this._call('Ask', text));
    }

    async confirm(id, index) {
        return JSON.parse(await this._call('Confirm', id, index));
    }

    dismiss(id) {
        this._call('Dismiss', id).catch(() => {});
    }

    shellResult(action, ok, message) {
        this._call('ShellResult', action, ok, message).catch(() => {});
    }

    async models() {
        return JSON.parse(await this._call('GetModels'));
    }

    async download(id) {
        await (await this._ready).DownloadAsync(id);
    }

    destroy() {
        if (this._proxy) {
            for (const id of this._signals)
                this._proxy.disconnectSignal(id);
        }
        this._signals = [];
        this._listeners.clear();
        this._proxy = null;
    }
}
