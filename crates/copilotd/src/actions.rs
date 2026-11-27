use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use cactus::{FunctionCall, Tool};
use serde::Serialize;
use serde_json::{Map, Value, json};
use tokio::process::Command;

use crate::apps;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Safe,

    Sensitive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Daemon,

    Shell,
}

pub struct Action {
    pub name: &'static str,

    pub label: &'static str,
    pub description: &'static str,
    pub risk: Risk,
    pub target: Target,
    pub parameters: fn() -> Value,
    pub triggers: &'static [&'static str],
}

impl Action {
    pub fn tool(&self) -> Tool {
        Tool {
            name: self.name.into(),
            description: self.description.into(),
            parameters: (self.parameters)(),
            triggers: self.triggers.iter().map(|s| s.to_string()).collect(),
        }
    }
}

fn no_params() -> Value {
    json!({"type": "object", "properties": {}})
}

fn toggle(description: &str) -> Value {
    json!({"type": "object", "properties": {
        "enabled": {"type": "boolean", "description": description}
    }, "required": ["enabled"]})
}

pub const SETTINGS_PANELS: &[&str] = &[
    "wifi", "network", "bluetooth", "display", "sound", "power", "background",
    "notifications", "keyboard", "mouse", "printers", "privacy", "apps",
    "online-accounts", "multitasking", "accessibility", "users", "system", "wacom",
];

pub const FOLDERS: &[&str] =
    &["home", "downloads", "documents", "pictures", "music", "videos", "desktop"];

pub static ACTIONS: &[Action] = &[
    Action {
        name: "open_app",
        label: "Abrir aplicativos",
        description: "Open (launch, start) an installed application by its name. Abrir, iniciar ou executar um aplicativo",
        risk: Risk::Safe,
        target: Target::Daemon,
        parameters: || json!({"type": "object", "properties": {
            "name": {"type": "string", "description": "Application name, e.g. Firefox, Files, Terminal"}
        }, "required": ["name"]}),
        triggers: &[],
    },
    Action {
        name: "open_settings",
        label: "Abrir configurações",
        description: "Open a page of GNOME Settings (configurações, ajustes, wallpaper = background)",
        risk: Risk::Safe,
        target: Target::Daemon,
        parameters: || json!({"type": "object", "properties": {
            "page": {"type": "string", "enum": SETTINGS_PANELS, "description": "Settings page"}
        }, "required": ["page"]}),
        triggers: &[r"configura(c|ç)(o|õ)es", r"\bajustes\b", r"\bsettings\b", r"papel de parede|plano de fundo|wallpaper"],
    },
    Action {
        name: "open_folder",
        label: "Abrir pastas",
        description: "Open a personal folder in the file manager (abrir pasta)",
        risk: Risk::Safe,
        target: Target::Daemon,
        parameters: || json!({"type": "object", "properties": {
            "folder": {"type": "string", "enum": FOLDERS}
        }, "required": ["folder"]}),
        triggers: &[r"\b(pasta|folder)\b", r"\b(meus|minhas|my) (documentos|downloads|imagens|fotos|m(ú|u)sicas|v(í|i)deos|documents|pictures|music|videos)\b"],
    },
    Action {
        name: "open_url",
        label: "Abrir sites",
        description: "Open a website in the web browser (abrir site)",
        risk: Risk::Safe,
        target: Target::Daemon,
        parameters: || json!({"type": "object", "properties": {
            "url": {"type": "string", "description": "Web address, e.g. https://gnome.org"}
        }, "required": ["url"]}),
        triggers: &[r"\.(com|org|net|br|io|dev|app)\b", r"\b(site|website|youtube|google|gmail|github|wikipedia)\b"],
    },
    Action {
        name: "set_volume",
        label: "Volume",
        description: "Set the speaker output volume to a percentage (volume)",
        risk: Risk::Safe,
        target: Target::Daemon,
        parameters: || json!({"type": "object", "properties": {
            "percent": {"type": "integer", "minimum": 0, "maximum": 100}
        }, "required": ["percent"]}),
        triggers: &[r"\bvolume\b"],
    },
    Action {
        name: "set_mute",
        label: "Silenciar som",
        description: "Mute or unmute the speakers (silenciar, mudo)",
        risk: Risk::Safe,
        target: Target::Daemon,
        parameters: || json!({"type": "object", "properties": {
            "muted": {"type": "boolean"}
        }, "required": ["muted"]}),
        triggers: &[r"\b(mudo|muda o som|silenci|mute|unmute)"],
    },
    Action {
        name: "set_dark_mode",
        label: "Estilo escuro",
        description: "Switch the desktop between dark style and light style (modo escuro, tema claro)",
        risk: Risk::Safe,
        target: Target::Daemon,
        parameters: || toggle("true for dark style, false for light style"),
        triggers: &[r"\b(modo|tema|estilo) (escuro|claro)\b", r"\b(dark|light) (mode|theme|style)\b"],
    },
    Action {
        name: "set_night_light",
        label: "Luz noturna",
        description: "Turn night light (warm screen colors) on or off (luz noturna)",
        risk: Risk::Safe,
        target: Target::Daemon,
        parameters: || toggle("true to turn night light on"),
        triggers: &[r"luz noturna|night ?light"],
    },
    Action {
        name: "set_do_not_disturb",
        label: "Não perturbe",
        description: "Turn do not disturb (silence notifications) on or off (não perturbe)",
        risk: Risk::Safe,
        target: Target::Daemon,
        parameters: || toggle("true to silence notifications"),
        triggers: &[r"n(ã|a)o perturbe|do not disturb"],
    },
    Action {
        name: "take_screenshot",
        label: "Captura de tela",
        description: "Take a screenshot (print screen, captura de tela)",
        risk: Risk::Safe,
        target: Target::Shell,
        parameters: no_params,
        triggers: &[r"\b(print|screenshot|captura)"],
    },
    Action {
        name: "close_app",
        label: "Fechar aplicativos",
        description: "Close (quit) a running application by its name (fechar app)",
        risk: Risk::Sensitive,
        target: Target::Shell,
        parameters: || json!({"type": "object", "properties": {
            "name": {"type": "string", "description": "Application name"}
        }, "required": ["name"]}),
        triggers: &[r"\b(fech(a|ar|e)|close|quit|kill)\b"],
    },
    Action {
        name: "lock_screen",
        label: "Bloquear tela",
        description: "Lock the screen (bloquear tela)",
        risk: Risk::Sensitive,
        target: Target::Daemon,
        parameters: no_params,
        triggers: &[r"\b(bloquei?a|bloquear|bloqueie|trava|travar|trave)\b", r"\block\b"],
    },
    Action {
        name: "power",
        label: "Desligar, reiniciar e sair",
        description: "Power off, restart the computer, or log out of the session (desligar, reiniciar, sair)",
        risk: Risk::Sensitive,
        target: Target::Daemon,
        parameters: || json!({"type": "object", "properties": {
            "operation": {"type": "string", "enum": ["power_off", "restart", "log_out"]}
        }, "required": ["operation"]}),
        triggers: &[r"\bdeslig(a|ar|ue) (o |a )?(computador|pc|notebook|m(á|a)quina|sistema)", r"\breinici", r"\b(restart|reboot|shut ?down|power ?off|log ?out|sign out)\b", r"\b(sair|encerrar) (da|a) sess"],
    },
    Action {
        name: "run_command",
        label: "Executar comandos no terminal",
        description: "Run a shell command in the terminal",
        risk: Risk::Sensitive,
        target: Target::Daemon,
        parameters: || json!({"type": "object", "properties": {
            "command": {"type": "string"}
        }, "required": ["command"]}),
        triggers: &[],
    },
];

pub fn find(name: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|a| a.name == name)
}

const ALWAYS_SENSITIVE: &[&str] = &["run_command"];

pub fn always_confirm(name: &str) -> bool {
    ALWAYS_SENSITIVE.contains(&name)
}

fn str_arg<'a>(args: &'a Map<String, Value>, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("argumento `{key}` ausente"))
}

fn bool_arg(args: &Map<String, Value>, key: &str) -> Result<bool> {
    match args.get(key) {
        Some(Value::Bool(b)) => Ok(*b),
        Some(Value::String(s)) => Ok(matches!(s.as_str(), "true" | "on" | "1")),
        _ => bail!("argumento `{key}` ausente"),
    }
}

fn percent_arg(args: &Map<String, Value>, key: &str) -> Result<u8> {
    let n = match args.get(key) {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim_end_matches('%').trim().parse().ok(),
        _ => None,
    }
    .ok_or_else(|| anyhow!("argumento `{key}` ausente"))?;
    Ok(n.clamp(0.0, 100.0).round() as u8)
}

fn one_of<'a>(value: &'a str, allowed: &[&str]) -> Result<&'a str> {
    allowed
        .iter()
        .find(|a| a.eq_ignore_ascii_case(value))
        .map(|_| value)
        .ok_or_else(|| anyhow!("opção desconhecida: {value}"))
}

pub fn describe(call: &FunctionCall) -> String {
    let a = &call.arguments;
    let s = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or("?").to_string();
    let on = |k: &str| a.get(k).and_then(Value::as_bool).unwrap_or(false);
    match call.name.as_str() {
        "open_app" => {
            let name = s("name");
            let shown = apps::find(&name).map(|app| app.name).unwrap_or(name);
            format!("Abrir {shown}")
        }
        "open_settings" => format!("Abrir configurações de {}", panel_label(&s("page"))),
        "open_folder" => format!("Abrir pasta {}", folder_label(&s("folder"))),
        "open_url" => format!("Abrir {}", s("url")),
        "set_volume" => format!(
            "Volume em {}%",
            a.get("percent").map(|v| v.to_string().trim_matches('"').to_string()).unwrap_or_default()
        ),
        "set_mute" => if on("muted") { "Silenciar som" } else { "Ativar som" }.into(),
        "set_dark_mode" => if on("enabled") { "Ativar estilo escuro" } else { "Ativar estilo claro" }.into(),
        "set_night_light" => if on("enabled") { "Ligar luz noturna" } else { "Desligar luz noturna" }.into(),
        "set_do_not_disturb" => if on("enabled") { "Ativar não perturbe" } else { "Desativar não perturbe" }.into(),
        "take_screenshot" => "Capturar a tela".into(),
        "close_app" => format!("Fechar {}", s("name")),
        "lock_screen" => "Bloquear a tela".into(),
        "power" => match s("operation").as_str() {
            "restart" => "Reiniciar o computador",
            "log_out" => "Encerrar a sessão",
            _ => "Desligar o computador",
        }
        .into(),
        "run_command" => format!("Executar: {}", s("command")),
        other => other.into(),
    }
}

fn panel_label(panel: &str) -> &str {
    match panel {
        "wifi" => "Wi-Fi",
        "network" => "rede",
        "bluetooth" => "Bluetooth",
        "display" => "tela",
        "sound" => "som",
        "power" => "energia",
        "background" => "plano de fundo",
        "notifications" => "notificações",
        "keyboard" => "teclado",
        "mouse" => "mouse",
        "printers" => "impressoras",
        "privacy" => "privacidade",
        "apps" => "aplicativos",
        "online-accounts" => "contas online",
        "multitasking" => "multitarefa",
        "accessibility" => "acessibilidade",
        "users" => "usuários",
        "system" => "sistema",
        other => other,
    }
}

fn folder_label(folder: &str) -> &str {
    match folder {
        "home" => "pessoal",
        "downloads" => "Downloads",
        "documents" => "Documentos",
        "pictures" => "Imagens",
        "music" => "Músicas",
        "videos" => "Vídeos",
        "desktop" => "Área de trabalho",
        other => other,
    }
}

pub fn validate(call: &FunctionCall) -> Result<()> {
    let a = &call.arguments;
    match call.name.as_str() {
        "open_app" => {
            let name = str_arg(a, "name")?;
            apps::find(name).ok_or_else(|| anyhow!("Não encontrei o app “{name}”"))?;
        }
        "close_app" => {
            str_arg(a, "name")?;
        }
        "open_settings" => {
            one_of(str_arg(a, "page")?, SETTINGS_PANELS)?;
        }
        "open_folder" => {
            one_of(str_arg(a, "folder")?, FOLDERS)?;
        }
        "open_url" => {
            normalize_url(str_arg(a, "url")?)?;
        }
        "set_volume" => {
            percent_arg(a, "percent")?;
        }
        "set_mute" => {
            bool_arg(a, "muted")?;
        }
        "set_dark_mode" | "set_night_light" | "set_do_not_disturb" => {
            bool_arg(a, "enabled")?;
        }
        "power" => {
            one_of(str_arg(a, "operation")?, &["power_off", "restart", "log_out"])?;
        }
        "run_command" => {
            str_arg(a, "command")?;
        }
        "take_screenshot" | "lock_screen" => {}
        other => bail!("ação desconhecida: {other}"),
    }
    Ok(())
}

fn normalize_url(raw: &str) -> Result<String> {
    let url = if raw.contains("://") { raw.to_string() } else { format!("https://{raw}") };
    let ok = (url.starts_with("https://") || url.starts_with("http://"))
        && !url.chars().any(char::is_whitespace)
        && url.len() > "https://".len() + 2;
    if !ok {
        bail!("endereço inválido: {raw}");
    }
    Ok(url)
}

async fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .await
        .with_context(|| format!("não foi possível executar {program}"))?;
    if !status.success() {
        bail!("{program} terminou com erro ({status})");
    }
    Ok(())
}

fn spawn(program: &str, args: &[&str]) -> Result<()> {
    Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("não foi possível executar {program}"))?;
    Ok(())
}

async fn gsettings(schema: &str, key: &str, value: &str) -> Result<()> {
    run("gsettings", &["set", schema, key, value]).await
}

async fn user_dir(folder: &str) -> PathBuf {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let xdg = match folder {
        "downloads" => "DOWNLOAD",
        "documents" => "DOCUMENTS",
        "pictures" => "PICTURES",
        "music" => "MUSIC",
        "videos" => "VIDEOS",
        "desktop" => "DESKTOP",
        _ => return home,
    };
    match Command::new("xdg-user-dir").arg(xdg).output().await {
        Ok(out) if out.status.success() => {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if path.is_empty() { home } else { PathBuf::from(path) }
        }
        _ => home,
    }
}

pub async fn execute(call: &FunctionCall) -> Result<String> {
    validate(call)?;
    let a = &call.arguments;
    match call.name.as_str() {
        "open_app" => {
            let app = apps::find(str_arg(a, "name")?).expect("validated");
            spawn("gio", &["launch", path_str(&app.path)?])?;
            Ok(format!("{} aberto", app.name))
        }
        "open_settings" => {
            let page = str_arg(a, "page")?.to_ascii_lowercase();
            spawn("gnome-control-center", &[&page])?;
            Ok(format!("Configurações de {} abertas", panel_label(&page)))
        }
        "open_folder" => {
            let folder = str_arg(a, "folder")?.to_ascii_lowercase();
            let dir = user_dir(&folder).await;
            run("gio", &["open", path_str(&dir)?]).await?;
            Ok(format!("Pasta {} aberta", folder_label(&folder)))
        }
        "open_url" => {
            let url = normalize_url(str_arg(a, "url")?)?;
            run("gio", &["open", &url]).await?;
            Ok(format!("{url} aberto no navegador"))
        }
        "set_volume" => {
            let p = percent_arg(a, "percent")?;
            let level = format!("{:.2}", f32::from(p) / 100.0);
            run("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "0"]).await.ok();
            run("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &level]).await?;
            Ok(format!("Volume em {p}%"))
        }
        "set_mute" => {
            let muted = bool_arg(a, "muted")?;
            run("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", if muted { "1" } else { "0" }]).await?;
            Ok(if muted { "Som silenciado" } else { "Som ativado" }.into())
        }
        "set_dark_mode" => {
            let on = bool_arg(a, "enabled")?;
            let scheme = if on { "'prefer-dark'" } else { "'default'" };
            gsettings("org.gnome.desktop.interface", "color-scheme", scheme).await?;
            Ok(if on { "Estilo escuro ativado" } else { "Estilo claro ativado" }.into())
        }
        "set_night_light" => {
            let on = bool_arg(a, "enabled")?;
            gsettings(
                "org.gnome.settings-daemon.plugins.color",
                "night-light-enabled",
                if on { "true" } else { "false" },
            )
            .await?;
            Ok(if on { "Luz noturna ligada" } else { "Luz noturna desligada" }.into())
        }
        "set_do_not_disturb" => {
            let on = bool_arg(a, "enabled")?;
            gsettings(
                "org.gnome.desktop.notifications",
                "show-banners",
                if on { "false" } else { "true" },
            )
            .await?;
            Ok(if on { "Não perturbe ativado" } else { "Não perturbe desativado" }.into())
        }
        "lock_screen" => {
            run("loginctl", &["lock-session"]).await?;
            Ok("Tela bloqueada".into())
        }
        "power" => {
            let flag = match str_arg(a, "operation")? {
                "restart" => "--reboot",
                "log_out" => "--logout",
                _ => "--power-off",
            };
            spawn("gnome-session-quit", &[flag])?;
            Ok("Aguardando sua confirmação no diálogo do sistema".into())
        }
        "run_command" => {
            let command = str_arg(a, "command")?;
            let out = Command::new("sh").arg("-c").arg(command).output().await?;
            let text = String::from_utf8_lossy(if out.status.success() { &out.stdout } else { &out.stderr });

            let mut body: String = text.trim_end().chars().take(4000).collect();
            if text.chars().count() > 4000 {
                body.push_str("\n…");
            }
            if !out.status.success() {
                bail!("O comando falhou: {body}");
            }
            Ok(if body.is_empty() { "Comando executado".into() } else { body })
        }
        "take_screenshot" | "close_app" => bail!("ação executada pelo GNOME Shell"),
        other => bail!("ação desconhecida: {other}"),
    }
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str().ok_or_else(|| anyhow!("caminho inválido"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: Value) -> FunctionCall {
        FunctionCall { name: name.into(), arguments: args.as_object().unwrap().clone() }
    }

    #[test]
    fn every_action_has_a_valid_schema() {
        for a in ACTIONS {
            let tool = a.tool();
            assert_eq!(tool.parameters["type"], "object", "{}", a.name);
            assert!(!a.description.is_empty());
        }
    }

    #[test]
    fn risk_levels() {
        assert_eq!(find("open_app").unwrap().risk, Risk::Safe);
        assert_eq!(find("power").unwrap().risk, Risk::Sensitive);
        assert_eq!(find("close_app").unwrap().target, Target::Shell);
        assert!(always_confirm("run_command"));
    }

    #[test]
    fn validates_arguments() {
        assert!(validate(&call("set_volume", json!({"percent": 30}))).is_ok());
        assert!(validate(&call("set_volume", json!({"percent": "30%"}))).is_ok());
        assert!(validate(&call("set_volume", json!({}))).is_err());
        assert!(validate(&call("open_settings", json!({"page": "wifi"}))).is_ok());
        assert!(validate(&call("open_settings", json!({"page": "rm -rf"}))).is_err());
        assert!(validate(&call("open_url", json!({"url": "gnome.org"}))).is_ok());
        assert!(validate(&call("open_url", json!({"url": "file:///etc/passwd"}))).is_err());
        assert!(validate(&call("power", json!({"operation": "explode"}))).is_err());
        assert!(validate(&call("nope", json!({}))).is_err());
    }

    #[test]
    fn describes_calls() {
        assert_eq!(describe(&call("set_volume", json!({"percent": 30}))), "Volume em 30%");
        assert_eq!(describe(&call("set_dark_mode", json!({"enabled": true}))), "Ativar estilo escuro");
        assert_eq!(
            describe(&call("open_settings", json!({"page": "wifi"}))),
            "Abrir configurações de Wi-Fi"
        );
    }
}
