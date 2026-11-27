use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Actions,

    Speech,

    Vision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Artifact {
    pub repo: &'static str,
    pub revision: &'static str,
    pub file: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

impl Artifact {
    pub fn url(&self) -> String {
        format!("https://huggingface.co/{}/resolve/{}/{}", self.repo, self.revision, self.file)
    }

    pub fn file_name(&self) -> &'static str {
        self.file.rsplit('/').next().unwrap_or(self.file)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Model {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ModelKind,
    pub summary: &'static str,
    pub artifact: Artifact,

    pub supported: bool,
}

pub const ENGINE: Artifact = Artifact {
    repo: "Cactus-Compute/cactus",
    revision: "f4ab2741203a282c6e7b89983de2b5e5b2e805a8",
    file: "releases/4.1.4/python/cactus_runtime-4.1.4-py3-none-manylinux2014_x86_64.whl",
    sha256: "314cd027e95cae1cb8537c478ca6cddaec3fd02165f80d960f1934399871fc33",
    size: 881_626,
};
pub const ENGINE_VERSION: &str = "4.1.4";
pub const ENGINE_MEMBER: &str = "cactus/libcactus.so";
pub const ENGINE_LIB: &str = "libcactus.so";

pub const MODELS: &[Model] = &[
    Model {
        id: "needle3",
        name: "Needle 3",
        kind: ModelKind::Actions,
        summary: "Entende pedidos e executa ações no sistema",
        artifact: Artifact {
            repo: "Cactus-Compute/needle3",
            revision: "d6c7d6a2e9655505e0f8e23339356dd1705900eb",
            file: "needle3.cact",
            sha256: "c9d915eca282ed42d1a09b143b592adb4cc6744ffe2d294adf5cfc5548170c38",
            size: 35_335_380,
        },
        supported: true,
    },
    Model {
        id: "whistle",
        name: "Whistle",
        kind: ModelKind::Speech,
        summary: "Comandos por voz e ditado",
        artifact: Artifact {
            repo: "Cactus-Compute/whistle",
            revision: "88cb00eeb8928ddbcd410b491584f938ba030b07",
            file: "whistle.cact",
            sha256: "b6e02f048568ac5d01a2042556c658061e699acbc0aa2a1439f52f3d461dffeb",
            size: 0,
        },
        supported: false,
    },
    Model {
        id: "mirage",
        name: "Mirage",
        kind: ModelKind::Vision,
        summary: "Enxerga a tela e interage com apps",
        artifact: Artifact {
            repo: "Cactus-Compute/mirage",
            revision: "76c1e9e53381aca8fc3843c636216c96ff220122",
            file: "mirage.cact",
            sha256: "4249a0f7515272ff76b9414bec17d7e7daf84b15b66c7fb4f17fce1e66f9f237",
            size: 0,
        },
        supported: false,
    },
];

pub fn find(id: &str) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_pinned() {
        for (i, m) in MODELS.iter().enumerate() {
            assert!(MODELS[i + 1..].iter().all(|o| o.id != m.id));
            assert_eq!(m.artifact.revision.len(), 40);
            assert_eq!(m.artifact.sha256.len(), 64);
        }
        assert_eq!(ENGINE.file_name(), "cactus_runtime-4.1.4-py3-none-manylinux2014_x86_64.whl");
        assert!(find("needle3").unwrap().supported);
    }
}
