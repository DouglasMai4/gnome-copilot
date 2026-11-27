use copilot_core::{BUS_NAME, INTERFACE, OBJECT_PATH};
use gtk::gio;
use gtk::glib::{self, Variant};
use gtk::prelude::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub summary: String,
    pub size: u64,
    pub supported: bool,
    pub installed: bool,
    pub active: bool,
    pub progress: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActionInfo {
    pub name: String,
    pub label: String,
    pub risk: String,
    pub enabled: bool,
    pub always_confirm: bool,
}

#[derive(Clone)]
pub struct Daemon {
    proxy: gio::DBusProxy,
}

pub enum Event {
    ModelsChanged,
    Progress(String, f64),
    Finished(#[allow(dead_code)] String, bool, String),
}

impl Daemon {
    pub async fn connect() -> Result<Self, glib::Error> {
        let proxy = gio::DBusProxy::for_bus_future(
            gio::BusType::Session,
            gio::DBusProxyFlags::empty(),
            None,
            BUS_NAME,
            OBJECT_PATH,
            INTERFACE,
        )
        .await?;
        Ok(Self { proxy })
    }

    async fn call(&self, method: &str, args: Option<Variant>) -> Result<Variant, glib::Error> {
        self.proxy
            .call_future(method, args.as_ref(), gio::DBusCallFlags::NONE, 30_000)
            .await
    }

    async fn call_string(&self, method: &str) -> Result<String, glib::Error> {
        let reply = self.call(method, None).await?;
        Ok(reply.get::<(String,)>().map(|(s,)| s).unwrap_or_default())
    }

    pub async fn models(&self) -> Result<Vec<ModelInfo>, glib::Error> {
        let json = self.call_string("GetModels").await?;
        Ok(serde_json::from_str(&json).unwrap_or_default())
    }

    pub async fn actions(&self) -> Result<Vec<ActionInfo>, glib::Error> {
        let json = self.call_string("GetActions").await?;
        Ok(serde_json::from_str(&json).unwrap_or_default())
    }

    pub async fn download(&self, id: &str) -> Result<(), glib::Error> {
        self.call("Download", Some((id,).to_variant())).await.map(drop)
    }

    pub async fn remove(&self, id: &str) -> Result<(), glib::Error> {
        self.call("RemoveModel", Some((id,).to_variant())).await.map(drop)
    }

    pub async fn set_active(&self, id: &str) -> Result<(), glib::Error> {
        self.call("SetActiveModel", Some((id,).to_variant())).await.map(drop)
    }

    pub async fn set_action_enabled(&self, name: &str, enabled: bool) -> Result<(), glib::Error> {
        self.call("SetActionEnabled", Some((name, enabled).to_variant())).await.map(drop)
    }

    pub async fn confirm_sensitive(&self) -> bool {
        let args = (INTERFACE, "ConfirmSensitive").to_variant();
        match self.call("org.freedesktop.DBus.Properties.Get", Some(args)).await {
            Ok(v) => v
                .get::<(Variant,)>()
                .and_then(|(inner,)| inner.get::<bool>())
                .unwrap_or(true),
            Err(_) => true,
        }
    }

    pub async fn set_confirm_sensitive(&self, value: bool) -> Result<(), glib::Error> {
        let args = (INTERFACE, "ConfirmSensitive", value.to_variant()).to_variant();
        self.call("org.freedesktop.DBus.Properties.Set", Some(args)).await.map(drop)
    }

    pub fn on_event(&self, callback: impl Fn(Event) + 'static) {
        self.proxy.connect_local("g-signal", false, move |values| {
            let name: String = values[2].get().ok()?;
            let params: Variant = values[3].get().ok()?;
            let event = match name.as_str() {
                "ModelsChanged" => Some(Event::ModelsChanged),
                "DownloadProgress" => params.get::<(String, f64)>().map(|(id, f)| Event::Progress(id, f)),
                "DownloadFinished" => params
                    .get::<(String, bool, String)>()
                    .map(|(id, ok, err)| Event::Finished(id, ok, err)),
                _ => None,
            };
            if let Some(event) = event {
                callback(event);
            }
            None
        });
    }
}

pub fn error_text(e: &glib::Error) -> String {
    let msg = e.message();
    msg.rsplit_once(": ").map(|(_, m)| m).unwrap_or(msg).to_string()
}
