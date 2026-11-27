import Atk from 'gi://Atk';
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const HIT_WIDTH = 18;
const HEIGHT = 88;
const PILL_WIDTH = 5;
const PILL_WIDTH_HOVER = 8;
const PILL_INSET = 3;
const DRAG_THRESHOLD = 6;
const SNAP_TIME = 260;
const HOVER_TIME = 180;

export const Notch = GObject.registerClass({
    Signals: {'activate': {}},
}, class Notch extends St.Widget {
    constructor(settings) {
        super({
            name: 'copilot-notch',
            style_class: 'copilot-notch',
            reactive: true,
            track_hover: true,
            can_focus: true,
            width: HIT_WIDTH,
            height: HEIGHT,
            accessible_name: 'Copilot',
            accessible_role: Atk.Role.PUSH_BUTTON,
        });
        this._settings = settings;
        this._pill = new St.Widget({
            style_class: 'copilot-notch-pill',
            width: PILL_WIDTH,
            height: HEIGHT,
        });
        this.add_child(this._pill);

        this._interface = new Gio.Settings({schema_id: 'org.gnome.desktop.interface'});
        this._interface.connectObject('changed::color-scheme', () => this._updateScheme(), this);
        this._updateScheme();

        this._drag = null;
        this._grab = null;

        this.connect('notify::hover', () => this._updatePill());
        this.connect('key-focus-in', () => this._updatePill());
        this.connect('key-focus-out', () => this._updatePill());

        this._settings.connectObject(
            'changed::notch-side', () => this.place(true),
            'changed::notch-position', () => {
                if (!this._drag)
                    this.place(true);
            },
            this);
        Main.layoutManager.connectObject(
            'monitors-changed', () => this.place(false),
            'hot-corners-changed', () => this.place(false),
            this);
    }

    _updateScheme() {
        const dark = this._interface.get_string('color-scheme') === 'prefer-dark';
        this._pill.remove_style_class_name(dark ? 'copilot-notch-light' : 'copilot-notch-dark');
        this._pill.add_style_class_name(dark ? 'copilot-notch-dark' : 'copilot-notch-light');
    }

    get side() {
        return this._settings.get_string('notch-side') === 'left' ? St.Side.LEFT : St.Side.RIGHT;
    }

    _workArea() {
        const index = Main.layoutManager.primaryIndex;
        return Main.layoutManager.getWorkAreaForMonitor(index);
    }

    _edgeX(side) {
        const area = this._workArea();
        return side === St.Side.LEFT ? area.x : area.x + area.width - HIT_WIDTH;
    }

    _pillX(side) {
        const wide = this.hover || this.has_key_focus() || this._drag?.moving;
        const w = wide ? PILL_WIDTH_HOVER : PILL_WIDTH;
        return side === St.Side.LEFT ? PILL_INSET : HIT_WIDTH - PILL_INSET - w;
    }

    _updatePill() {
        const wide = this.hover || this.has_key_focus() || this._drag?.moving;
        this._pill.ease({
            width: wide ? PILL_WIDTH_HOVER : PILL_WIDTH,
            x: this._pillX(this.side),
            duration: HOVER_TIME,
            mode: Clutter.AnimationMode.EASE_OUT_QUAD,
        });
        if (wide)
            this._pill.add_style_pseudo_class('hover');
        else
            this._pill.remove_style_pseudo_class('hover');
    }

    place(animate) {
        const area = this._workArea();
        const fraction = this._settings.get_double('notch-position');
        const x = this._edgeX(this.side);
        const y = Math.round(area.y + fraction * Math.max(0, area.height - HEIGHT));
        this._pill.x = this._pillX(this.side);
        if (animate && this.mapped) {
            this.ease({x, y, duration: SNAP_TIME, mode: Clutter.AnimationMode.EASE_OUT_EXPO});
        } else {
            this.remove_all_transitions();
            this.set_position(x, y);
        }
    }

    _start(event) {
        const [px, py] = event.get_coords();
        this._drag = {startX: px, startY: py, offsetY: py - this.y, moving: false};
        this._grab = global.stage.grab(this);
        return Clutter.EVENT_STOP;
    }

    _move(event) {
        if (!this._drag)
            return Clutter.EVENT_PROPAGATE;
        const [px, py] = event.get_coords();
        const d = this._drag;
        if (!d.moving && Math.hypot(px - d.startX, py - d.startY) < DRAG_THRESHOLD)
            return Clutter.EVENT_STOP;
        if (!d.moving) {
            d.moving = true;
            this._updatePill();
        }
        const area = this._workArea();
        const y = Math.min(Math.max(py - d.offsetY, area.y), area.y + area.height - HEIGHT);

        const x = Math.min(Math.max(px - HIT_WIDTH / 2, area.x), area.x + area.width - HIT_WIDTH);
        this.remove_all_transitions();
        this.set_position(x, y);
        return Clutter.EVENT_STOP;
    }

    _end(event) {
        if (!this._drag)
            return Clutter.EVENT_PROPAGATE;
        const d = this._drag;
        this._grab?.dismiss();
        this._grab = null;
        this._drag = null;
        if (!d.moving) {
            this._updatePill();
            this.emit('activate');
            return Clutter.EVENT_STOP;
        }
        const area = this._workArea();
        const [px] = event.get_coords();
        const side = px < area.x + area.width / 2 ? 'left' : 'right';
        const fraction = (this.y - area.y) / Math.max(1, area.height - HEIGHT);
        this._settings.set_double('notch-position', Math.min(Math.max(fraction, 0), 1));
        if (this._settings.get_string('notch-side') !== side)
            this._settings.set_string('notch-side', side);
        else
            this.place(true);
        this._updatePill();
        return Clutter.EVENT_STOP;
    }

    vfunc_button_press_event(event) {
        if (event.get_button() !== Clutter.BUTTON_PRIMARY)
            return Clutter.EVENT_PROPAGATE;
        return this._start(event);
    }

    vfunc_motion_event(event) {
        return this._move(event);
    }

    vfunc_button_release_event(event) {
        return this._end(event);
    }

    vfunc_touch_event(event) {
        switch (event.type()) {
        case Clutter.EventType.TOUCH_BEGIN:
            return this._start(event);
        case Clutter.EventType.TOUCH_UPDATE:
            return this._move(event);
        case Clutter.EventType.TOUCH_END:
        case Clutter.EventType.TOUCH_CANCEL:
            return this._end(event);
        default:
            return Clutter.EVENT_PROPAGATE;
        }
    }

    vfunc_key_press_event(event) {
        const key = event.get_key_symbol();
        if ([Clutter.KEY_Return, Clutter.KEY_KP_Enter, Clutter.KEY_space].includes(key)) {
            this.emit('activate');
            return Clutter.EVENT_STOP;
        }
        return super.vfunc_key_press_event(event);
    }

    destroy() {
        this._interface?.disconnectObject(this);
        this._interface = null;
        this._grab?.dismiss();
        this._grab = null;
        super.destroy();
    }
});
