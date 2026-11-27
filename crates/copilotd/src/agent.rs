use std::path::Path;

use anyhow::Result;
use cactus::{Completion, Engine, FunctionCall, Tool};
use copilot_core::config::Config;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::actions::{self, Risk, Target};

pub const NOT_UNDERSTOOD: f32 = 0.2;

struct Loaded {
    engine: Engine,
    model: String,
    tools: Vec<String>,
}

#[derive(Default)]
pub struct Agent {
    loaded: Option<Loaded>,
}

impl Agent {
    pub fn unload(&mut self) {
        self.loaded = None;
    }

    pub fn propose(
        &mut self,
        engine_path: &Path,
        model_id: &str,
        weights: &Path,
        threads: u32,
        tools: &[Tool],
        text: &str,
    ) -> Result<Completion> {
        let names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
        let reload = self.loaded.as_ref().is_none_or(|l| l.model != model_id);
        if reload {
            self.loaded = None;
            let mut engine = Engine::open(engine_path)?;
            engine.set_threads(threads);
            engine.load_weights(weights)?;
            engine.init("", tools)?;
            self.loaded = Some(Loaded { engine, model: model_id.into(), tools: names });
        } else if let Some(l) = self.loaded.as_mut().filter(|l| l.tools != names) {
            l.engine.init("", tools)?;
            l.tools = names;
        }
        let loaded = self.loaded.as_mut().expect("loaded above");

        loaded.engine.reset();
        Ok(loaded.engine.complete(text, 256)?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Done,

    Shell,

    NeedsConfirmation,

    Suggestion,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub index: u32,
    pub action: String,
    pub title: String,
    pub arguments: Map<String, Value>,
    pub risk: Risk,
    pub status: Status,
    pub message: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Run,
    Confirm,
    Suggest,
}

pub fn decide(action: &actions::Action, confidence: Option<f32>, cfg: &Config) -> Decision {
    if confidence.is_some_and(|c| c < cfg.confidence_floor) {
        return Decision::Suggest;
    }
    if action.risk == Risk::Sensitive && (cfg.confirm_sensitive || actions::always_confirm(action.name)) {
        return Decision::Confirm;
    }
    Decision::Run
}

pub fn plan_item(index: u32, call: &FunctionCall, confidence: Option<f32>, cfg: &Config) -> Item {
    let mut item = Item {
        index,
        action: call.name.clone(),
        title: actions::describe(call),
        arguments: call.arguments.clone(),
        risk: Risk::Safe,
        status: Status::Failed,
        message: String::new(),
        app_id: None,
    };
    if matches!(call.name.as_str(), "open_app" | "close_app") {
        item.app_id = call
            .arguments
            .get("name")
            .and_then(Value::as_str)
            .and_then(crate::apps::find)
            .map(|app| app.id);
    }
    let Some(action) = actions::find(&call.name).filter(|a| cfg.action_enabled(a.name)) else {
        item.message = "Essa ação está desativada nas configurações".into();
        return item;
    };
    item.risk = action.risk;
    if let Err(e) = actions::validate(call) {
        item.message = e.to_string();
        return item;
    }
    item.status = match decide(action, confidence, cfg) {
        Decision::Suggest => Status::Suggestion,
        Decision::Confirm => Status::NeedsConfirmation,
        Decision::Run if action.target == Target::Shell => Status::Shell,

        Decision::Run => Status::Done,
    };
    item
}

pub async fn run_item(item: &mut Item, call: &FunctionCall) {
    match actions::find(&call.name).map(|a| a.target) {
        Some(Target::Shell) => {
            item.status = Status::Shell;
        }
        Some(Target::Daemon) => match actions::execute(call).await {
            Ok(message) => {
                item.status = Status::Done;
                item.message = message;
            }
            Err(e) => {
                item.status = Status::Failed;
                item.message = e.to_string();
            }
        },
        None => {
            item.status = Status::Failed;
            item.message = "ação desconhecida".into();
        }
    }
}

pub fn enabled_tools(cfg: &Config) -> Vec<Tool> {
    actions::ACTIONS.iter().filter(|a| cfg.action_enabled(a.name)).map(|a| a.tool()).collect()
}

pub fn ground(query: &str, completion: &Completion) -> Vec<(FunctionCall, Option<f32>)> {
    dedupe(completion.function_calls.clone())
        .into_iter()
        .map(|mut call| {
            crate::hints::refine(query, &mut call);
            let confidence = if crate::hints::triggered(query, &call.name) || resolves_to_app(&call) {
                Some(1.0)
            } else {
                completion.confidence
            };
            (call, confidence)
        })
        .collect()
}

fn resolves_to_app(call: &FunctionCall) -> bool {
    call.name == "open_app"
        && call.arguments.get("name").and_then(Value::as_str).is_some_and(|n| crate::apps::find(n).is_some())
}

pub fn dedupe(calls: Vec<FunctionCall>) -> Vec<FunctionCall> {
    let mut out: Vec<FunctionCall> = Vec::new();
    for c in calls {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(name: &str, args: Value) -> FunctionCall {
        FunctionCall { name: name.into(), arguments: args.as_object().unwrap().clone() }
    }

    #[test]
    fn policy() {
        let cfg = Config::default();
        let safe = actions::find("set_volume").unwrap();
        let sensitive = actions::find("lock_screen").unwrap();
        assert_eq!(decide(safe, Some(0.9), &cfg), Decision::Run);
        assert_eq!(decide(safe, Some(0.35), &cfg), Decision::Suggest);
        assert_eq!(decide(sensitive, Some(0.9), &cfg), Decision::Confirm);
        let relaxed = Config { confirm_sensitive: false, ..Config::default() };
        assert_eq!(decide(sensitive, Some(0.9), &relaxed), Decision::Run);
        let command = actions::find("run_command").unwrap();
        assert_eq!(decide(command, Some(0.99), &relaxed), Decision::Confirm);
    }

    #[test]
    fn items() {
        let cfg = Config::default();
        let i = plan_item(0, &call("set_dark_mode", json!({"enabled": true})), Some(0.6), &cfg);
        assert_eq!(i.status, Status::Done);
        assert_eq!(i.title, "Ativar estilo escuro");
        let i = plan_item(0, &call("take_screenshot", json!({})), Some(0.9), &cfg);
        assert_eq!(i.status, Status::Shell);
        let i = plan_item(0, &call("run_command", json!({"command": "ls"})), Some(0.9), &cfg);
        assert_eq!(i.status, Status::Failed, "disabled by default");
        let i = plan_item(0, &call("set_volume", json!({})), Some(0.9), &cfg);
        assert_eq!(i.status, Status::Failed);
    }

    #[test]
    fn tools_respect_config() {
        let tools = enabled_tools(&Config::default());
        assert!(tools.iter().any(|t| t.name == "open_app"));
        assert!(!tools.iter().any(|t| t.name == "run_command"));
    }
}

#[cfg(test)]
mod eval {
    use super::*;

    const CASES: &[(&str, &str)] = &[
        ("abre o firefox", "open_app"),
        ("abrir o terminal", "open_app"),
        ("open the files app", "open_app"),
        ("inicia o spotify", "open_app"),
        ("abre as configurações de wi-fi", "open_settings"),
        ("quero mudar o papel de parede", "open_settings"),
        ("configurações de bluetooth", "open_settings"),
        ("abre a pasta downloads", "open_folder"),
        ("mostra meus documentos", "open_folder"),
        ("abre o youtube", "open_url"),
        ("vai para github.com", "open_url"),
        ("coloca o volume em 30", "set_volume"),
        ("volume no máximo", "set_volume"),
        ("set volume to 50 percent", "set_volume"),
        ("silencia o som", "set_mute"),
        ("tira o mudo", "set_mute"),
        ("ativa o modo escuro", "set_dark_mode"),
        ("muda para o tema claro", "set_dark_mode"),
        ("liga a luz noturna", "set_night_light"),
        ("ativa o não perturbe", "set_do_not_disturb"),
        ("tira um print", "take_screenshot"),
        ("captura a tela", "take_screenshot"),
        ("fecha o spotify", "close_app"),
        ("close firefox", "close_app"),
        ("bloqueia a tela", "lock_screen"),
        ("lock my computer", "lock_screen"),
        ("desliga o computador", "power"),
        ("reinicia o pc", "power"),
        ("sair da sessão", "power"),

        ("qual a capital da frança?", "-"),
        ("me conta uma piada", "-"),
        ("quanto é 2 mais 2", "-"),
        ("obrigado!", "-"),
        ("o que você consegue fazer?", "-"),
    ];

    #[test]
    fn eval_intents() {
        let (Ok(lib), Ok(weights)) = (std::env::var("CACTUS_LIB"), std::env::var("NEEDLE_WEIGHTS"))
        else {
            return;
        };
        let tools = enabled_tools(&Config::default());
        let mut agent = Agent::default();
        let mut hits = 0;
        for (query, expected) in CASES {
            let c = agent.propose(lib.as_ref(), "needle3", weights.as_ref(), 0, &tools, query).unwrap();
            let grounded = ground(query, &c);
            let first = grounded.first();
            let got = first.map(|(f, _)| f.name.as_str()).unwrap_or("-");
            let conf = first.and_then(|(_, c)| *c).unwrap_or(0.0);

            let ok = if *expected == "-" {
                conf < Config::default().confidence_floor
            } else {
                got == *expected && conf >= Config::default().confidence_floor
            };
            hits += ok as usize;
            let args = first.map(|(f, _)| serde_json::to_string(&f.arguments).unwrap()).unwrap_or_default();
            println!("{} {:<40} {:<18} {:.2} {}", if ok { "✓" } else { "✗" }, query, got, conf, args);
        }
        println!("accuracy: {hits}/{}", CASES.len());
    }
}
