use std::ffi::{CStr, CString, c_char, c_int, c_uchar, c_ulonglong};
use std::path::Path;
use std::ptr;

use serde::{Deserialize, Serialize};

pub const MODEL_TEXT: i32 = 1;
pub const MODEL_SPEECH: i32 = 2;
pub const MODEL_VISION: i32 = 4;

const OUTPUT_CAPACITY: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not load the Cactus engine: {0}")]
    Library(#[from] libloading::Error),
    #[error("the Cactus engine is missing the `{0}` symbol (incompatible version)")]
    Symbol(&'static str),
    #[error("could not read model weights: {0}")]
    Io(#[from] std::io::Error),
    #[error("{call} failed: {message}")]
    Engine { call: &'static str, message: String },
    #[error("text contains a NUL byte")]
    Nul(#[from] std::ffi::NulError),
    #[error("the engine returned an unreadable response: {0}")]
    Response(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

type LoadFn = unsafe extern "C" fn(*const c_uchar, c_ulonglong) -> c_int;
type ModelsFn = unsafe extern "C" fn() -> c_int;
type LastErrorFn = unsafe extern "C" fn() -> *const c_char;
type InitFn = unsafe extern "C" fn(*const c_char, *const c_char, *const c_char) -> c_int;
type CompleteFn =
    unsafe extern "C" fn(*const c_char, *const f32, c_int, c_int, *mut c_char, c_int) -> c_int;
type SetThreadsFn = unsafe extern "C" fn(c_int);
type ResetFn = unsafe extern "C" fn();

pub struct Engine {
    load: LoadFn,
    models: ModelsFn,
    last_error: LastErrorFn,
    init: InitFn,
    complete: CompleteFn,
    set_threads: SetThreadsFn,
    reset: ResetFn,
    output: Vec<u8>,

    _lib: libloading::Library,
}

// SAFETY: the engine has no thread affinity; `&mut self` serializes every call.
unsafe impl Send for Engine {}

macro_rules! symbol {
    ($lib:expr, $name:literal) => {{
        // SAFETY: the signature matches cactus.h for engine 4.x.
        let symbol = unsafe { $lib.get(concat!($name, "\0").as_bytes()) }
            .map_err(|_| Error::Symbol($name))?;
        *symbol
    }};
}

impl Engine {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        // SAFETY: pinned, checksum-verified engine; its initializers have no preconditions.
        let lib = unsafe { libloading::Library::new(path.as_ref()) }?;
        Ok(Self {
            load: symbol!(lib, "cactus_load"),
            models: symbol!(lib, "cactus_models"),
            last_error: symbol!(lib, "cactus_last_error"),
            init: symbol!(lib, "cactus_init"),
            complete: symbol!(lib, "cactus_complete"),
            set_threads: symbol!(lib, "cactus_set_threads"),
            reset: symbol!(lib, "cactus_reset"),
            output: vec![0; OUTPUT_CAPACITY],
            _lib: lib,
        })
    }

    pub fn set_threads(&mut self, threads: u32) {
        // SAFETY: plain value argument.
        unsafe { (self.set_threads)(threads.min(i32::MAX as u32) as c_int) }
    }

    pub fn load_weights(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let data = std::fs::read(path)?;

        // SAFETY: pointer and length describe `data`, which outlives the call.
        let code = unsafe { (self.load)(data.as_ptr(), data.len() as c_ulonglong) };
        if code < 0 {
            return Err(self.engine_error("cactus_load"));
        }
        Ok(())
    }

    pub fn models(&mut self) -> i32 {
        // SAFETY: no arguments.
        unsafe { (self.models)() }
    }

    pub fn init(&mut self, system: &str, tools: &[Tool]) -> Result<i32> {
        let system = CString::new(system)?;
        let tools = CString::new(serde_json::to_string(tools)?)?;

        // SAFETY: NUL-terminated strings; a null tool index is allowed.
        let code = unsafe { (self.init)(system.as_ptr(), tools.as_ptr(), ptr::null()) };
        if code < 0 {
            return Err(self.engine_error("cactus_init"));
        }
        Ok(code)
    }

    pub fn reset(&mut self) {
        // SAFETY: no arguments.
        unsafe { (self.reset)() }
    }

    pub fn complete(&mut self, text: &str, max_new_tokens: u32) -> Result<Completion> {
        let raw = self.complete_raw(text, max_new_tokens)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn complete_raw(&mut self, text: &str, max_new_tokens: u32) -> Result<String> {
        let input = CString::new(text)?;
        self.output.fill(0);

        // SAFETY: `input` is NUL-terminated; buffer pointer and capacity match.
        let code = unsafe {
            (self.complete)(
                input.as_ptr(),
                ptr::null(),
                0,
                max_new_tokens.min(i32::MAX as u32) as c_int,
                self.output.as_mut_ptr().cast(),
                self.output.len() as c_int,
            )
        };
        let written = CStr::from_bytes_until_nul(&self.output)
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if code < 0 {
            let message = if written.is_empty() { self.last_error_message() } else { written };
            return Err(Error::Engine { call: "cactus_complete", message });
        }
        Ok(written)
    }

    fn last_error_message(&self) -> String {
        // SAFETY: returns an engine-owned C string or null.
        let ptr = unsafe { (self.last_error)() };
        if ptr.is_null() {
            return "unknown error".into();
        }

        // SAFETY: non-null and NUL-terminated; valid until the next engine call.
        unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
    }

    fn engine_error(&self, call: &'static str) -> Error {
        Error::Engine { call, message: self.last_error_message() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tool {
    pub name: String,
    pub description: String,

    pub parameters: serde_json::Value,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionCall {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Completion {
    #[serde(rename = "type")]
    pub kind: String,
    pub success: bool,
    pub error: Option<String>,
    pub function_calls: Vec<FunctionCall>,
    pub suppressed_calls: Vec<FunctionCall>,
    pub reasoning: Option<String>,
    pub confidence: Option<f32>,

    pub response: Option<String>,

    pub prefill_tps: Option<f32>,

    pub decode_tps: Option<f32>,
    pub peak_ram_mb: Option<f32>,
}

impl Completion {
    pub fn is_call(&self) -> bool {
        self.kind == "call" && !self.function_calls.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_envelope() {
        let raw = r#"{"type":"call","success":true,"error":null,"error_code":null,"reason":null,
            "function_calls":[{"name":"open_app","arguments":{"name":"Firefox"}}],
            "suppressed_calls":[],"reasoning":"'firefox' -> name 'Firefox'","confidence":0.9607,
            "prefill_tps":360.1,"decode_tps":178.9,"peak_ram_mb":105.6,
            "validation":{"ungrounded":[],"negation":false}}"#;
        let c: Completion = serde_json::from_str(raw).unwrap();
        assert!(c.is_call());
        assert_eq!(c.function_calls[0].name, "open_app");
        assert_eq!(c.function_calls[0].arguments["name"], "Firefox");
        assert!((c.confidence.unwrap() - 0.9607).abs() < 1e-4);
    }

    #[test]
    fn tolerates_missing_fields() {
        let c: Completion = serde_json::from_str(r#"{"type":"text"}"#).unwrap();
        assert!(!c.is_call());
    }

    #[test]
    fn live_engine_roundtrip() {
        let (Ok(lib), Ok(weights)) = (std::env::var("CACTUS_LIB"), std::env::var("NEEDLE_WEIGHTS"))
        else {
            return;
        };
        let mut engine = Engine::open(lib).unwrap();
        engine.load_weights(weights).unwrap();
        assert!(engine.models() & MODEL_TEXT != 0);
        let tool = Tool {
            name: "open_app".into(),
            description: "Open an installed application by name".into(),
            parameters: serde_json::json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}),
            triggers: vec![],
        };
        engine.init("", &[tool]).unwrap();
        let c = engine.complete("open firefox", 128).unwrap();
        assert_eq!(c.function_calls[0].name, "open_app");
    }
}
