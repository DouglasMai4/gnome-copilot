use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use cactus::FunctionCall;
use copilot_core::catalog::{self, ModelKind};
use copilot_core::config::Config;
use serde_json::{Value, json};
use tracing::{info, warn};
use zbus::fdo;
use zbus::object_server::SignalEmitter;

use crate::actions;
use crate::agent::{self, Agent, Item, Status};
use crate::models;

const MAX_PENDING: usize = 32;

#[derive(Default)]
struct State {
    config: Config,

    downloads: HashMap<String, f64>,

    pending: HashMap<String, Vec<(u32, FunctionCall)>>,
    next_id: u64,
}

impl State {
    fn status(&self) -> &'static str {
        if !self.downloads.is_empty() {
            return "downloading";
        }
        let ready = self
            .config
            .active_model
            .as_deref()
            .and_then(catalog::find)
            .is_some_and(|m| models::is_installed(m) && models::engine_installed());
        if ready { "ready" } else { "no_model" }
    }

    fn save(&self) {
        if let Err(e) = self.config.save() {
            warn!("could not save config: {e:#}");
        }
    }
}

#[derive(Clone)]
pub struct Service {
    state: Arc<Mutex<State>>,
    agent: Arc<Mutex<Agent>>,
    http: reqwest::Client,
}

impl Service {
    pub fn new() -> Self {
        let mut config = Config::load();

        if let Some(id) = config.active_model.clone()
            && !catalog::find(&id).is_some_and(models::is_installed)
        {
            config.active_model = None;
        }
        Self {
            state: Arc::new(Mutex::new(State { config, ..State::default() })),
            agent: Arc::default(),
            http: models::client(),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    async fn notify(&self, emitter: &SignalEmitter<'_>) {
        let status = self.state().status();
        Self::models_changed(emitter).await.ok();
        Self::status_changed(emitter, status).await.ok();
    }

    async fn run_download(&self, emitter: SignalEmitter<'static>, id: String) {
        let model = catalog::find(&id).expect("checked by caller");
        let result = async {
            models::ensure_engine(&self.http).await?;
            let mut last = 0.0;
            let service = self.clone();
            let emitter2 = emitter.clone();
            let id2 = id.clone();
            models::download(&self.http, &model.artifact, &models::model_path(model), move |done, total| {
                let fraction = if total > 0 { done as f64 / total as f64 } else { 0.0 };

                if fraction - last >= 0.01 || fraction >= 1.0 {
                    last = fraction;
                    service.state().downloads.insert(id2.clone(), fraction);
                    let (e, i) = (emitter2.clone(), id2.clone());
                    tokio::spawn(async move { Service::download_progress(&e, &i, fraction).await.ok() });
                }
            })
            .await
        }
        .await;

        {
            let mut st = self.state();
            st.downloads.remove(&id);
            if result.is_ok() && model.kind == ModelKind::Actions && st.config.active_model.is_none() {
                st.config.active_model = Some(id.clone());
                st.save();
            }
        }
        let error = result.as_ref().err().map(|e| format!("{e:#}")).unwrap_or_default();
        match &result {
            Ok(()) => info!("downloaded {id}"),
            Err(_) => warn!("download of {id} failed: {error}"),
        }
        Self::download_finished(&emitter, &id, result.is_ok(), &error).await.ok();
        self.notify(&emitter).await;
    }
}

fn reply(value: Value) -> String {
    value.to_string()
}

fn err(message: impl Into<String>) -> fdo::Error {
    fdo::Error::Failed(message.into())
}

#[zbus::interface(name = "io.github.GnomeCopilot1")]
impl Service {
    async fn get_status(&self) -> String {
        self.state().status().into()
    }

    async fn ask(&self, text: String) -> fdo::Result<String> {
        let text = text.trim().to_string();
        if text.is_empty() {
            return Err(err("pedido vazio"));
        }
        let cfg = self.state().config.clone();
        let model = cfg
            .active_model
            .as_deref()
            .and_then(catalog::find)
            .filter(|m| models::is_installed(m))
            .ok_or_else(|| err("Nenhum modelo instalado. Baixe o Needle nas configurações."))?;

        let tools = agent::enabled_tools(&cfg);
        let agent = self.agent.clone();
        let weights = models::model_path(model);
        let engine = models::engine_path();
        let (model_id, threads, query) = (model.id.to_string(), cfg.threads, text.clone());
        let started = std::time::Instant::now();
        let completion = tokio::task::spawn_blocking(move || {
            let mut agent = agent.lock().unwrap_or_else(|e| e.into_inner());
            agent.propose(&engine, &model_id, &weights, threads, &tools, &query)
        })
        .await
        .map_err(|e| err(e.to_string()))?
        .map_err(|e| err(format!("O modelo falhou: {e:#}")))?;

        let inference_ms = started.elapsed().as_millis() as u64;
        info!(confidence = ?completion.confidence, calls = completion.function_calls.len(), inference_ms, "ask");

        let stats = |total_ms: u64| {
            json!({
                "model": model.name,
                "inference_ms": inference_ms,
                "total_ms": total_ms,
                "confidence": completion.confidence,
                "prefill_tps": completion.prefill_tps,
                "decode_tps": completion.decode_tps,
                "peak_ram_mb": completion.peak_ram_mb,
                "reasoning": completion.reasoning,
            })
        };
        let grounded = agent::ground(&text, &completion);
        let understood = grounded.iter().any(|(_, c)| c.is_none_or(|c| c >= agent::NOT_UNDERSTOOD));
        if !completion.is_call() || !understood {
            let message = completion
                .response
                .filter(|r| !r.trim().is_empty())
                .unwrap_or_else(|| "Tente algo como “abrir o Firefox” ou “ativar o estilo escuro”.".into());
            let total = started.elapsed().as_millis() as u64;
            return Ok(reply(json!({"id": "", "items": [], "message": message, "stats": stats(total)})));
        }

        let mut items: Vec<Item> = Vec::new();
        let mut waiting = Vec::new();
        for (i, (call, confidence)) in grounded.iter().enumerate() {
            let mut item = agent::plan_item(i as u32, call, *confidence, &cfg);
            match item.status {
                Status::Done => agent::run_item(&mut item, call).await,
                Status::NeedsConfirmation | Status::Suggestion => waiting.push((i as u32, call.clone())),
                Status::Shell | Status::Failed => {}
            }
            items.push(item);
        }

        let id = {
            let mut st = self.state();
            st.next_id += 1;
            let id = st.next_id.to_string();
            if !waiting.is_empty() {
                if st.pending.len() >= MAX_PENDING {
                    st.pending.clear();
                }
                st.pending.insert(id.clone(), waiting);
            }
            id
        };
        let total = started.elapsed().as_millis() as u64;
        Ok(reply(json!({"id": id, "items": items, "message": "", "stats": stats(total)})))
    }

    async fn confirm(&self, id: String, index: u32) -> fdo::Result<String> {
        let call = {
            let mut st = self.state();
            let list = st.pending.get_mut(&id).ok_or_else(|| err("esse pedido expirou"))?;
            let pos = list.iter().position(|(i, _)| *i == index).ok_or_else(|| err("ação não encontrada"))?;
            let (_, call) = list.remove(pos);
            if list.is_empty() {
                st.pending.remove(&id);
            }
            call
        };
        let cfg = self.state().config.clone();
        let mut item = agent::plan_item(index, &call, None, &cfg);
        if item.status != Status::Failed {
            agent::run_item(&mut item, &call).await;
        }
        Ok(reply(json!(item)))
    }

    async fn dismiss(&self, id: String) {
        self.state().pending.remove(&id);
    }

    async fn shell_result(&self, action: String, ok: bool, message: String) {
        info!(%action, ok, %message, "shell action");
    }

    async fn get_models(&self) -> String {
        let st = self.state();
        let list: Vec<Value> = catalog::MODELS
            .iter()
            .map(|m| {
                json!({
                    "id": m.id,
                    "name": m.name,
                    "kind": m.kind,
                    "summary": m.summary,
                    "size": m.artifact.size,
                    "supported": m.supported,
                    "installed": models::is_installed(m),
                    "active": st.config.active_model.as_deref() == Some(m.id),
                    "progress": st.downloads.get(m.id).copied(),
                })
            })
            .collect();
        reply(json!(list))
    }

    async fn download(
        &self,
        id: String,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let model = catalog::find(&id).ok_or_else(|| err("modelo desconhecido"))?;
        if !model.supported {
            return Err(err("esse modelo chega numa próxima versão"));
        }
        {
            let mut st = self.state();
            if st.downloads.contains_key(&id) {
                return Ok(());
            }
            st.downloads.insert(id.clone(), 0.0);
        }
        self.notify(&emitter).await;
        let service = self.clone();
        let emitter = emitter.to_owned();
        tokio::spawn(async move { service.run_download(emitter, id).await });
        Ok(())
    }

    async fn remove_model(
        &self,
        id: String,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let model = catalog::find(&id).ok_or_else(|| err("modelo desconhecido"))?;
        {
            let mut st = self.state();
            if st.config.active_model.as_deref() == Some(model.id) {
                st.config.active_model = None;
                st.save();
            }
        }
        self.agent.lock().unwrap_or_else(|e| e.into_inner()).unload();
        models::remove(model).map_err(|e| err(e.to_string()))?;
        self.notify(&emitter).await;
        Ok(())
    }

    async fn set_active_model(
        &self,
        id: String,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let model = catalog::find(&id).ok_or_else(|| err("modelo desconhecido"))?;
        if model.kind != ModelKind::Actions || !models::is_installed(model) {
            return Err(err("esse modelo não pode ser ativado"));
        }
        {
            let mut st = self.state();
            st.config.active_model = Some(model.id.into());
            st.save();
        }
        self.notify(&emitter).await;
        Ok(())
    }

    async fn get_actions(&self) -> String {
        let st = self.state();
        let list: Vec<Value> = actions::ACTIONS
            .iter()
            .map(|a| {
                json!({
                    "name": a.name,
                    "label": a.label,
                    "risk": a.risk,
                    "enabled": st.config.action_enabled(a.name),
                    "always_confirm": actions::always_confirm(a.name),
                })
            })
            .collect();
        reply(json!(list))
    }

    async fn set_action_enabled(&self, name: String, enabled: bool) -> fdo::Result<()> {
        actions::find(&name).ok_or_else(|| err("ação desconhecida"))?;
        let mut st = self.state();
        st.config.disabled_actions.retain(|n| n != &name);
        if !enabled {
            st.config.disabled_actions.push(name);
        }
        st.save();
        Ok(())
    }

    #[zbus(property)]
    async fn confirm_sensitive(&self) -> bool {
        self.state().config.confirm_sensitive
    }

    #[zbus(property)]
    async fn set_confirm_sensitive(&mut self, value: bool) {
        let mut st = self.state();
        st.config.confirm_sensitive = value;
        st.save();
    }

    #[zbus(signal)]
    async fn status_changed(emitter: &SignalEmitter<'_>, status: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn models_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn download_progress(emitter: &SignalEmitter<'_>, id: &str, fraction: f64) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn download_finished(emitter: &SignalEmitter<'_>, id: &str, ok: bool, error: &str) -> zbus::Result<()>;
}
