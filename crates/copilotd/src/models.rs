use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use copilot_core::catalog::{self, Artifact, Model};
use copilot_core::paths;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

pub fn model_path(model: &Model) -> PathBuf {
    paths::models_dir()
        .join(model.id)
        .join(model.artifact.revision)
        .join(model.artifact.file_name())
}

pub fn is_installed(model: &Model) -> bool {
    let Ok(meta) = std::fs::metadata(model_path(model)) else { return false };
    meta.is_file() && (model.artifact.size == 0 || meta.len() == model.artifact.size)
}

pub fn engine_path() -> PathBuf {
    paths::engine_dir().join(catalog::ENGINE_VERSION).join(catalog::ENGINE_LIB)
}

pub fn engine_installed() -> bool {
    engine_path().is_file()
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("gnome-copilot/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(20))
        .build()
        .expect("TLS backend available")
}

pub async fn download(
    client: &reqwest::Client,
    artifact: &Artifact,
    dest: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<()> {
    let dir = dest.parent().context("caminho de destino inválido")?;
    tokio::fs::create_dir_all(dir).await?;
    let part = dest.with_extension("part");

    let response = client
        .get(artifact.url())
        .send()
        .await
        .context("sem conexão com o Hugging Face")?
        .error_for_status()
        .context("o Hugging Face recusou o download")?;
    let total = response.content_length().unwrap_or(artifact.size);

    let mut file = tokio::fs::File::create(&part).await?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("a conexão caiu durante o download")?;
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        progress(done, total);
    }
    file.flush().await?;
    drop(file);

    let digest = hex::encode(hasher.finalize());
    if digest != artifact.sha256 {
        tokio::fs::remove_file(&part).await.ok();
        bail!("o arquivo baixado não confere com a assinatura esperada");
    }
    tokio::fs::rename(&part, dest).await?;
    Ok(())
}

pub async fn ensure_engine(client: &reqwest::Client) -> Result<PathBuf> {
    let target = engine_path();
    if target.is_file() {
        return Ok(target);
    }
    let dir = target.parent().context("caminho do motor inválido")?.to_path_buf();
    let wheel = dir.join(catalog::ENGINE.file_name());
    download(client, &catalog::ENGINE, &wheel, |_, _| {}).await?;

    let wheel_for_task = wheel.clone();
    let target_for_task = target.clone();
    tokio::task::spawn_blocking(move || extract_engine(&wheel_for_task, &target_for_task))
        .await??;
    tokio::fs::remove_file(&wheel).await.ok();
    Ok(target)
}

fn extract_engine(wheel: &Path, target: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(wheel)?)?;
    let mut member = archive
        .by_name(catalog::ENGINE_MEMBER)
        .context("o pacote do motor não contém libcactus.so")?;
    let mut bytes = Vec::with_capacity(member.size() as usize);
    member.read_to_end(&mut bytes)?;
    let tmp = target.with_extension("so.part");
    let mut out = std::fs::File::create(&tmp)?;
    out.write_all(&bytes)?;
    out.sync_all()?;
    std::fs::rename(tmp, target)?;
    Ok(())
}

pub fn remove(model: &Model) -> Result<()> {
    let dir = paths::models_dir().join(model.id);
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_versioned() {
        let m = catalog::find("needle3").unwrap();
        let p = model_path(m);
        assert!(p.ends_with("needle3/d6c7d6a2e9655505e0f8e23339356dd1705900eb/needle3.cact"));
        assert!(engine_path().ends_with("engine/4.1.4/libcactus.so"));
    }
}
