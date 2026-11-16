//! The optional local voice engine (relay/install.sh, install_voice.sh, install_dutch_voice.sh).
//!
//! The engines themselves stay Python because they are third-party machine-learning code with no Rust equivalent:
//!   relay/claude_relay.py   the voice sidecar on 127.0.0.1:47602 (speech-to-text / text-to-speech HTTP endpoints; the
//!                           session/hook/stats parts of that file are superseded by the Rust hearth-relay)
//!   relay/nl_tts_server.py  the optional Dutch voice server
//!   pip packages: kokoro-onnx, onnx-asr[cpu,hub], faster-whisper, piper-tts (and coqui-tts, torch, torchaudio, transformers<4.50 for Dutch)
//!   models: Parakeet TDT 0.6B v3 (int8), Kokoro 82M, Piper en_US lessac and nl_BE nathalie, Coqui XTTS-v2
//! This module only installs them: venv, packages, model files, systemd user units.
use crate::sysx::{have, Env};
use crate::texts;
use crate::ui::*;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const CLAUDE_RELAY_PY: &str = include_str!("../../../relay/claude_relay.py");
pub const NL_TTS_PY: &str = include_str!("../../../relay/nl_tts_server.py");

const KOKORO: &str = "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0";
const PIPER_EN: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium";
const PIPER_NL: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/main/nl/nl_BE/nathalie/medium";
pub const PIP_PACKAGES: [&str; 4] = ["kokoro-onnx", "onnx-asr[cpu,hub]", "faster-whisper", "piper-tts"];

pub struct VoicePaths { pub base: PathBuf, pub venv: PathBuf, pub models: PathBuf, pub voices: PathBuf, pub unit: PathBuf }
pub fn paths(home: &Path) -> VoicePaths {
    let base = home.join(".local/share/hearth-relay");
    VoicePaths { venv: base.join("venv"), models: base.join("models"), voices: base.join("voices"), unit: home.join(".config/systemd/user/hearth-relay.service"), base }
}
/// (url, where it goes relative to the base folder)
pub fn model_downloads() -> Vec<(String, String)> {
    let mut v = vec![];
    for f in ["kokoro-v1.0.onnx", "voices-v1.0.bin"] { v.push((format!("{KOKORO}/{f}"), format!("models/{f}"))); }
    for f in ["en_US-lessac-medium.onnx", "en_US-lessac-medium.onnx.json"] { v.push((format!("{PIPER_EN}/{f}"), format!("voices/{f}"))); }
    for f in ["nl_BE-nathalie-medium.onnx", "nl_BE-nathalie-medium.onnx.json"] { v.push((format!("{PIPER_NL}/{f}"), format!("voices/{f}"))); }
    v
}

/// Brings the installed engine files up to date with the ones inside this program.
pub fn refresh_engine_files(base: &Path) -> Result<()> {
    std::fs::create_dir_all(base)?;
    write_exec(&base.join("claude_relay.py"), CLAUDE_RELAY_PY)?;
    if base.join("nl_tts_server.py").is_file() { write_exec(&base.join("nl_tts_server.py"), NL_TTS_PY)?; }
    Ok(())
}
fn write_exec(f: &Path, text: &str) -> Result<()> {
    let tmp = f.with_extension("py.new");
    std::fs::write(&tmp, text)?; crate::sysx::set_mode(&tmp, 0o755)?; std::fs::rename(&tmp, f)?;
    Ok(())
}

fn user_systemctl(on: bool, args: &[&str]) { if on { let _ = Command::new("systemctl").arg("--user").args(args).output(); } }

/// install.sh: the voice sidecar as a user service on 127.0.0.1:47602. Returns the unit text.
pub fn install_service(home: &Path, systemctl: bool) -> Result<String> {
    let p = paths(home);
    refresh_engine_files(&p.base)?;
    let venv_py = p.venv.join("bin/python");
    let py = if venv_py.is_file() { venv_py.to_string_lossy().to_string() } else { "/usr/bin/python3".to_string() };
    let unit = texts::voice_unit_text(&py, &p.base.to_string_lossy(), &home.to_string_lossy());
    if let Some(d) = p.unit.parent() { std::fs::create_dir_all(d)?; }
    std::fs::write(&p.unit, &unit)?;
    user_systemctl(systemctl, &["daemon-reload"]);
    user_systemctl(systemctl, &["enable", "hearth-relay.service"]);
    user_systemctl(systemctl, &["restart", "hearth-relay.service"]);
    if systemctl { let _ = Command::new("loginctl").args(["enable-linger", &crate::sysx::current_user()]).output(); }
    Ok(unit)
}

/// install_voice.sh rewrote the unit's ExecStart to use the venv; the same, on text.
pub fn patch_unit_exec(unit: &str, venv_py: &str, base: &str) -> String {
    unit.lines().map(|l| if l.starts_with("ExecStart=") && l.contains("claude_relay.py") { format!("ExecStart={venv_py} {base}/claude_relay.py") } else { l.to_string() }).collect::<Vec<_>>().join("\n") + "\n"
}

pub fn download(url: &str, dest: &Path) -> Result<()> {
    if std::fs::metadata(dest).map(|m| m.len() > 0).unwrap_or(false) { return Ok(()); }
    if let Some(d) = dest.parent() { std::fs::create_dir_all(d)?; }
    let mut r = reqwest::blocking::Client::builder().user_agent("Hearth-server").timeout(std::time::Duration::from_secs(3600)).build()?.get(url).send()?;
    if !r.status().is_success() { bail!("{url} said {}", r.status().as_u16()); }
    let part = dest.with_extension("part");
    { let mut f = std::fs::File::create(&part)?; r.copy_to(&mut f)?; }
    std::fs::rename(&part, dest)?;
    Ok(())
}

fn must_run(label: &str, cmd: &str, args: &[&str]) -> Result<()> {
    println!("  … {label}");
    let st = Command::new(cmd).args(args).status();
    match st { Ok(s) if s.success() => { ok(label); Ok(()) } _ => bail!("{label} did not work") }
}

/// install_voice.sh: venv, packages, models. Safe to re-run. Needs the network and a few GB of disk.
pub fn install_engines(home: &Path, systemctl: bool) -> Result<()> {
    let p = paths(home);
    for d in [&p.base, &p.models, &p.voices] { std::fs::create_dir_all(d)?; }
    if !have("espeak-ng") { warn("Note: Kokoro needs espeak-ng (sudo dnf install espeak-ng)."); }
    if !p.venv.is_dir() { must_run("Make the Python environment", "python3", &["-m", "venv", &p.venv.to_string_lossy()])?; }
    let pip = p.venv.join("bin/pip").to_string_lossy().to_string();
    must_run("Update pip", &pip, &["install", "--quiet", "--upgrade", "pip"])?;
    let mut args = vec!["install", "--quiet"]; args.extend(PIP_PACKAGES);
    must_run("Install the voice packages", &pip, &args)?;
    for (url, rel) in model_downloads() { let f = p.base.join(&rel); if !f.is_file() { println!("  … download {rel}"); } download(&url, &f)?; }
    println!("  … Downloading the speech recognition model (first run only, ~700 MB)");
    must_run("Load the speech model", &p.venv.join("bin/python").to_string_lossy(), &["-c", "import onnx_asr; onnx_asr.load_model('nemo-parakeet-tdt-0.6b-v3', quantization='int8'); print('speech model ready')"])?;
    if p.unit.is_file() {
        let t = std::fs::read_to_string(&p.unit)?;
        std::fs::write(&p.unit, patch_unit_exec(&t, &p.venv.join("bin/python").to_string_lossy(), &p.base.to_string_lossy()))?;
        user_systemctl(systemctl, &["daemon-reload"]);
        user_systemctl(systemctl, &["restart", "hearth-relay.service"]);
        ok("Relay restarted with the new voice engines.");
    } else { println!("  Run `hearth-server voice install` first, then re-run this."); }
    Ok(())
}

/// install_dutch_voice.sh: Coqui XTTS-v2 next to the normal engine. Asks about the licence first.
pub fn install_dutch(home: &Path, pr: &mut Prompt, systemctl: bool) -> Result<()> {
    let p = paths(home);
    let venv = p.base.join("venv-nl");
    let unit = home.join(".config/systemd/user/hearth-nl-tts.service");
    if !pr.confirm("XTTS-v2 is licensed for non-commercial use (Coqui Public Model License). Continue?", false) { bail!("Cancelled."); }
    std::fs::create_dir_all(&p.base)?;
    if !have("uv") { must_run("Install uv", "python3", &["-m", "pip", "install", "--user", "--quiet", "uv"])?; }
    let uv = crate::sysx::which("uv").unwrap_or_else(|| home.join(".local/bin/uv")).to_string_lossy().to_string();
    if !venv.is_dir() { must_run("Make the Python 3.12 environment", &uv, &["venv", "--python", "3.12", &venv.to_string_lossy()])?; }
    must_run("Install the Dutch voice packages", &uv, &["pip", "install", "--python", &venv.join("bin/python").to_string_lossy(), "--quiet", "coqui-tts", "transformers<4.50", "torch", "torchaudio", "--extra-index-url", "https://download.pytorch.org/whl/cpu"])?;
    write_exec(&p.base.join("nl_tts_server.py"), NL_TTS_PY)?;
    println!("  … Downloading the voice model (first time only)");
    let st = Command::new(venv.join("bin/python")).env("COQUI_TOS_AGREED", "1").args(["-c", "from TTS.api import TTS\nt = TTS('tts_models/multilingual/multi-dataset/xtts_v2')\nt.tts(text='Hallo, dit is een test.', speaker='Ana Florence', language='nl')\nprint('Dutch voice ready')"]).status();
    if !st.map(|s| s.success()).unwrap_or(false) { bail!("Downloading the Dutch voice model did not work"); }
    if let Some(d) = unit.parent() { std::fs::create_dir_all(d)?; }
    std::fs::write(&unit, texts::dutch_unit_text(&venv.to_string_lossy(), &p.base.to_string_lossy()))?;
    user_systemctl(systemctl, &["daemon-reload"]);
    user_systemctl(systemctl, &["enable", "--now", "hearth-nl-tts.service"]);
    user_systemctl(systemctl, &["restart", "hearth-relay.service"]);
    ok("Done. Dutch replies are now spoken with the new voice.");
    Ok(())
}

/// `voice install`: service + engines (Env is only used for --dry-run).
pub fn voice_install(env: &Env, home: &Path, pr: &mut Prompt, dutch: bool, engines: bool) -> Result<()> {
    if env.dry { println!("  {}", dim("[dry run] would install the voice service, python packages and models into ~/.local/share/hearth-relay")); return Ok(()); }
    let unit = install_service(home, true)?; let _ = unit; ok("Voice service installed (127.0.0.1:47602)");
    if engines { install_engines(home, true)?; }
    if dutch { install_dutch(home, pr, true)?; }
    if let Ok(t) = std::fs::read_to_string(home.join(".config/hearth-relay/token")) { println!("Pairing token: {}", t.trim()); }
    if have("firewall-cmd") { println!("\nFedora's firewall blocks the relay by default. Allow it on your home network zone with:\n  sudo firewall-cmd --permanent --add-port=47601/tcp --add-port=47600/udp && sudo firewall-cmd --reload"); }
    Ok(())
}
