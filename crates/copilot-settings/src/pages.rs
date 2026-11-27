use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::daemon::{self, Daemon, Event, ModelInfo};

const EXTENSION_UUID: &str = "gnome-copilot@gnome-copilot.github.io";
const EXTENSION_SCHEMA: &str = "org.gnome.shell.extensions.gnome-copilot";

struct Ui {
    toasts: adw::ToastOverlay,
    models_page: adw::PreferencesPage,
    models_groups: RefCell<Vec<adw::PreferencesGroup>>,
    actions_page: adw::PreferencesPage,
    actions_groups: RefCell<Vec<adw::PreferencesGroup>>,
    progress: RefCell<HashMap<String, (gtk::ProgressBar, gtk::Label)>>,
    daemon: RefCell<Option<Daemon>>,
}

impl Ui {
    fn toast(&self, text: &str) {
        self.toasts.add_toast(adw::Toast::new(text));
    }
}

pub fn build_window(app: &adw::Application, page: &str) -> adw::ApplicationWindow {
    let stack = adw::ViewStack::new();

    let general = general_page();
    stack.add_titled_with_icon(&general, Some("general"), "Geral", "preferences-system-symbolic");

    let models_page = adw::PreferencesPage::new();
    stack.add_titled_with_icon(&models_page, Some("models"), "Modelos", "folder-download-symbolic");

    let actions_page = adw::PreferencesPage::new();
    stack.add_titled_with_icon(&actions_page, Some("actions"), "Ações", "system-run-symbolic");

    let switcher = adw::ViewSwitcher::builder().stack(&stack).policy(adw::ViewSwitcherPolicy::Wide).build();
    let header = adw::HeaderBar::builder().title_widget(&switcher).build();
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&stack));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&toasts));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Copilot")
        .default_width(640)
        .default_height(720)
        .width_request(360)
        .content(&toolbar)
        .build();
    if stack.child_by_name(page).is_some() {
        stack.set_visible_child_name(page);
    }

    let ui = Rc::new(Ui {
        toasts,
        models_page,
        models_groups: RefCell::default(),
        actions_page,
        actions_groups: RefCell::default(),
        progress: RefCell::default(),
        daemon: RefCell::default(),
    });
    placeholder(&ui.models_page, &ui.models_groups, "Conectando ao serviço do Copilot…");
    placeholder(&ui.actions_page, &ui.actions_groups, "Conectando ao serviço do Copilot…");

    glib::spawn_future_local(connect(ui));
    window
}

async fn connect(ui: Rc<Ui>) {
    match Daemon::connect().await {
        Ok(d) => {
            let ui2 = ui.clone();
            d.on_event(move |event| on_event(&ui2, event));
            ui.daemon.replace(Some(d));
            refresh_models(ui.clone()).await;
            refresh_actions(ui).await;
        }
        Err(e) => {
            let text = format!("Não consegui falar com o serviço do Copilot ({})", daemon::error_text(&e));
            placeholder(&ui.models_page, &ui.models_groups, &text);
            placeholder(&ui.actions_page, &ui.actions_groups, &text);
        }
    }
}

fn on_event(ui: &Rc<Ui>, event: Event) {
    match event {
        Event::ModelsChanged => {
            glib::spawn_future_local(refresh_models(ui.clone()));
        }
        Event::Progress(id, fraction) => {
            if let Some((bar, label)) = ui.progress.borrow().get(&id) {
                bar.set_fraction(fraction);
                label.set_text(&format!("{}%", (fraction * 100.0).round()));
            }
        }
        Event::Finished(_, ok, error) => {
            if ok {
                ui.toast("Modelo baixado e pronto para usar");
            } else {
                ui.toast(&format!("O download falhou: {error}"));
            }
        }
    }
}

fn clear(page: &adw::PreferencesPage, groups: &RefCell<Vec<adw::PreferencesGroup>>) {
    for g in groups.borrow_mut().drain(..) {
        page.remove(&g);
    }
}

fn add(page: &adw::PreferencesPage, groups: &RefCell<Vec<adw::PreferencesGroup>>, group: adw::PreferencesGroup) {
    page.add(&group);
    groups.borrow_mut().push(group);
}

fn placeholder(page: &adw::PreferencesPage, groups: &RefCell<Vec<adw::PreferencesGroup>>, text: &str) {
    clear(page, groups);
    let group = adw::PreferencesGroup::new();
    let row = adw::ActionRow::builder().title(text).build();
    row.add_css_class("property");
    group.add(&row);
    add(page, groups, group);
}

fn extension_settings() -> Option<gio::Settings> {
    let lookup = |source: &gio::SettingsSchemaSource| source.lookup(EXTENSION_SCHEMA, true);
    if let Some(schema) = gio::SettingsSchemaSource::default().as_ref().and_then(lookup) {
        return Some(gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or(home.join(".local/share"));
    let candidates = [
        data_home.join("gnome-shell/extensions").join(EXTENSION_UUID).join("schemas"),
        PathBuf::from("/usr/share/gnome-shell/extensions").join(EXTENSION_UUID).join("schemas"),
    ];
    candidates.iter().find_map(|dir| {
        let source = gio::SettingsSchemaSource::from_directory(dir, gio::SettingsSchemaSource::default().as_ref(), false).ok()?;
        let schema = lookup(&source)?;
        Some(gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None))
    })
}

fn bind_combo(row: &adw::ComboRow, settings: &gio::Settings, key: &'static str, nicks: &'static [&'static str]) {
    let current = settings.string(key);
    row.set_selected(nicks.iter().position(|n| *n == current.as_str()).unwrap_or(0) as u32);
    let s = settings.clone();
    row.connect_selected_notify(move |row| {
        if let Some(nick) = nicks.get(row.selected() as usize) {
            s.set_string(key, nick).ok();
        }
    });
    let r = row.clone();
    settings.connect_changed(Some(key), move |s, _| {
        let value = s.string(key);
        if let Some(i) = nicks.iter().position(|n| *n == value.as_str())
            && r.selected() != i as u32 {
                r.set_selected(i as u32);
            }
    });
}

fn general_page() -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let access = adw::PreferencesGroup::builder()
        .title("Acesso")
        .description("Como o painel do Copilot é aberto. Clique fora dele para recolher.")
        .build();
    page.add(&access);

    match extension_settings() {
        Some(settings) => {
            let mode = adw::ComboRow::builder()
                .title("Abrir pelo")
                .model(&gtk::StringList::new(&["Notch na lateral da tela", "Ícone na barra superior", "Notch e ícone"]))
                .build();
            bind_combo(&mode, &settings, "trigger-mode", &["notch", "tray", "both"]);
            access.add(&mode);

            let side = adw::ComboRow::builder()
                .title("Lado do notch")
                .subtitle("Você também pode arrastar o notch para o outro lado")
                .model(&gtk::StringList::new(&["Esquerda", "Direita"]))
                .build();
            bind_combo(&side, &settings, "notch-side", &["left", "right"]);
            let update_side = {
                let side = side.clone();
                move |s: &gio::Settings| side.set_sensitive(s.string("trigger-mode") != "tray")
            };
            update_side(&settings);
            settings.connect_changed(Some("trigger-mode"), move |s, _| update_side(s));
            access.add(&side);

            let accel = settings.strv("toggle-shortcut").first().map(|s| s.to_string()).unwrap_or_default();
            let shortcut = adw::ActionRow::builder().title("Atalho de teclado").build();
            shortcut.add_suffix(&gtk::ShortcutLabel::builder().accelerator(accel).valign(gtk::Align::Center).build());
            access.add(&shortcut);
        }
        None => {
            let row = adw::ActionRow::builder()
                .title("Extensão do Copilot não encontrada")
                .subtitle("Instale e ative a extensão do GNOME Shell para usar o notch e o ícone")
                .build();
            row.add_prefix(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
            access.add(&row);
        }
    }

    let privacy = adw::PreferencesGroup::builder().title("Privacidade").build();
    let local = adw::ActionRow::builder()
        .title("Tudo roda no seu computador")
        .subtitle("Seus pedidos nunca saem desta máquina. A internet é usada só para baixar modelos do Hugging Face.")
        .build();
    local.add_prefix(&gtk::Image::from_icon_name("security-high-symbolic"));
    privacy.add(&local);
    page.add(&privacy);

    page
}

fn size_text(bytes: u64) -> String {
    if bytes == 0 {
        String::new()
    } else {
        format!("{} MB", (bytes as f64 / 1_000_000.0).round())
    }
}

async fn refresh_models(ui: Rc<Ui>) {
    let Some(d) = ui.daemon.borrow().clone() else { return };
    let models = match d.models().await {
        Ok(m) => m,
        Err(e) => {
            ui.toast(&daemon::error_text(&e));
            return;
        }
    };
    clear(&ui.models_page, &ui.models_groups);
    ui.progress.borrow_mut().clear();

    let available = adw::PreferencesGroup::builder()
        .title("Ações")
        .description("Os modelos são baixados do Hugging Face, conferidos por assinatura e ficam só no seu computador.")
        .build();
    let soon = adw::PreferencesGroup::builder()
        .title("Em breve")
        .description("Voz e visão chegam nas próximas versões.")
        .build();

    for m in &models {
        let row = model_row(&ui, &d, m);
        if m.supported { available.add(&row) } else { soon.add(&row) }
    }
    add(&ui.models_page, &ui.models_groups, available);
    add(&ui.models_page, &ui.models_groups, soon);
}

fn model_row(ui: &Rc<Ui>, d: &Daemon, m: &ModelInfo) -> adw::ActionRow {
    let subtitle = match size_text(m.size) {
        s if s.is_empty() => m.summary.clone(),
        s => format!("{} · {s}", m.summary),
    };
    let row = adw::ActionRow::builder().title(&m.name).subtitle(&subtitle).build();
    let icon = match m.kind.as_str() {
        "speech" => "audio-input-microphone-symbolic",
        "vision" => "view-reveal-symbolic",
        _ => "system-run-symbolic",
    };
    row.add_prefix(&gtk::Image::from_icon_name(icon));

    if !m.supported {
        let tag = gtk::Label::builder().label("Em breve").valign(gtk::Align::Center).build();
        tag.add_css_class("dim-label");
        row.add_suffix(&tag);
        return row;
    }

    if let Some(progress) = m.progress {
        let bar = gtk::ProgressBar::builder().fraction(progress).valign(gtk::Align::Center).width_request(120).build();
        let label = gtk::Label::builder()
            .label(format!("{}%", (progress * 100.0).round()))
            .width_chars(4)
            .xalign(1.0)
            .build();
        label.add_css_class("numeric");
        label.add_css_class("dim-label");
        row.add_suffix(&bar);
        row.add_suffix(&label);
        ui.progress.borrow_mut().insert(m.id.clone(), (bar, label));
        return row;
    }

    if !m.installed {
        let button = gtk::Button::builder().label("Baixar").valign(gtk::Align::Center).build();
        button.add_css_class("suggested-action");
        button.add_css_class("pill");
        let (ui2, d2, id) = (ui.clone(), d.clone(), m.id.clone());
        button.connect_clicked(move |b| {
            b.set_sensitive(false);
            let (ui, d, id) = (ui2.clone(), d2.clone(), id.clone());
            glib::spawn_future_local(async move {
                if let Err(e) = d.download(&id).await {
                    ui.toast(&daemon::error_text(&e));
                    refresh_models(ui).await;
                }
            });
        });
        row.add_suffix(&button);
        return row;
    }

    if m.active {
        let tag = gtk::Label::builder().label("Em uso").valign(gtk::Align::Center).build();
        tag.add_css_class("dim-label");
        row.add_suffix(&tag);
    } else {
        let use_button = gtk::Button::builder().label("Usar").valign(gtk::Align::Center).build();
        use_button.add_css_class("pill");
        let (ui2, d2, id) = (ui.clone(), d.clone(), m.id.clone());
        use_button.connect_clicked(move |_| {
            let (ui, d, id) = (ui2.clone(), d2.clone(), id.clone());
            glib::spawn_future_local(async move {
                if let Err(e) = d.set_active(&id).await {
                    ui.toast(&daemon::error_text(&e));
                }
            });
        });
        row.add_suffix(&use_button);
    }

    let remove = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .valign(gtk::Align::Center)
        .tooltip_text("Remover modelo")
        .build();
    remove.add_css_class("flat");
    remove.add_css_class("circular");
    let (ui2, d2, id, name) = (ui.clone(), d.clone(), m.id.clone(), m.name.clone());
    remove.connect_clicked(move |b| {
        let dialog = adw::AlertDialog::new(
            Some(&format!("Remover {name}?")),
            Some("O arquivo do modelo será apagado. Você pode baixá-lo de novo quando quiser."),
        );
        dialog.add_responses(&[("cancel", "Cancelar"), ("remove", "Remover")]);
        dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        let (ui, d, id) = (ui2.clone(), d2.clone(), id.clone());
        dialog.connect_response(None, move |_, response| {
            if response != "remove" {
                return;
            }
            let (ui, d, id) = (ui.clone(), d.clone(), id.clone());
            glib::spawn_future_local(async move {
                match d.remove(&id).await {
                    Ok(()) => ui.toast("Modelo removido"),
                    Err(e) => ui.toast(&daemon::error_text(&e)),
                }
            });
        });
        dialog.present(b.root().as_ref());
    });
    row.add_suffix(&remove);
    row
}

async fn refresh_actions(ui: Rc<Ui>) {
    let Some(d) = ui.daemon.borrow().clone() else { return };
    let actions = match d.actions().await {
        Ok(a) => a,
        Err(e) => {
            ui.toast(&daemon::error_text(&e));
            return;
        }
    };
    let confirm = d.confirm_sensitive().await;
    clear(&ui.actions_page, &ui.actions_groups);

    let safety = adw::PreferencesGroup::builder().title("Segurança").build();
    let confirm_row = adw::SwitchRow::builder()
        .title("Confirmar ações sensíveis")
        .subtitle("Pede sua confirmação antes de fechar apps, bloquear a tela ou desligar")
        .active(confirm)
        .build();
    let (ui2, d2) = (ui.clone(), d.clone());
    confirm_row.connect_active_notify(move |row| {
        let (ui, d, value) = (ui2.clone(), d2.clone(), row.is_active());
        glib::spawn_future_local(async move {
            if let Err(e) = d.set_confirm_sensitive(value).await {
                ui.toast(&daemon::error_text(&e));
            }
        });
    });
    safety.add(&confirm_row);
    add(&ui.actions_page, &ui.actions_groups, safety);

    let allowed = adw::PreferencesGroup::builder()
        .title("O que o Copilot pode fazer")
        .description("Ações desligadas não são oferecidas ao modelo.")
        .build();
    for a in actions {
        let subtitle = if a.always_confirm {
            "Sempre pede confirmação"
        } else if a.risk == "sensitive" {
            "Sensível"
        } else {
            ""
        };
        let row = adw::SwitchRow::builder().title(&a.label).active(a.enabled).build();
        if !subtitle.is_empty() {
            row.set_subtitle(subtitle);
        }
        let (ui2, d2, name) = (ui.clone(), d.clone(), a.name.clone());
        row.connect_active_notify(move |row| {
            let (ui, d, name, value) = (ui2.clone(), d2.clone(), name.clone(), row.is_active());
            glib::spawn_future_local(async move {
                if let Err(e) = d.set_action_enabled(&name, value).await {
                    ui.toast(&daemon::error_text(&e));
                }
            });
        });
        allowed.add(&row);
    }
    add(&ui.actions_page, &ui.actions_groups, allowed);
}
