use std::path::PathBuf;

#[derive(Clone)]
pub struct DataDirs {
    pub assets_dir: PathBuf,
    pub versions_dir: PathBuf,
    pub indexes_dir: PathBuf,
    pub objects_dir: PathBuf,
    pub pomme_assets_dir: PathBuf,
    pub bedrock_assets_dir: PathBuf,
    pub jar_assets_dir: PathBuf,
    pub game_dir: PathBuf,
    /// Version-specific mod home inside the active profile.
    pub mods_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub mods_dir: PathBuf,
}

impl DataDirs {
    pub fn resolve(
        version: &str,
        assets_dir: Option<&str>,
        versions_dir: Option<&str>,
        game_dir: Option<&str>,
    ) -> Self {
        let layout = pomme_dirs::layout();

        let assets_dir = assets_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| layout.minecraft_assets());

        let game_dir = game_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| layout.default_profile());

        let versions_dir = versions_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| layout.minecraft_versions());

        let jar_assets_dir = versions_dir.join(version).join("extracted/assets");
        let mod_version = pomme_protocol::ProtocolVersion::from_name(version)
            .map_or("unsupported", |version| version.name);
        let mods_dir = game_dir.join("mods").join(mod_version);

        let indexes_dir = assets_dir.join("indexes");
        let objects_dir = assets_dir.join("objects");

        Self {
            assets_dir,
            versions_dir,
            indexes_dir,
            objects_dir,
            jar_assets_dir,
            pomme_assets_dir: layout.pomme_assets(),
            bedrock_assets_dir: layout.bedrock_assets(),
            mods_dir: game_dir.join("mods").join(version),
            game_dir,
            mods_dir,
            cache_dir: layout.cache(version),
        }
    }

    pub fn ensure_game_dir(&self) -> std::io::Result<()> {
        pomme_dirs::create_profile(&self.game_dir)?;
        std::fs::create_dir_all(&self.mods_dir)
    }

    /// Each Minecraft version gets an isolated Java-mod folder.
    pub fn ensure_mods_dir(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.mods_dir)
    }

    /// `version`'s assets are downloaded, so the client can start on them.
    pub fn is_installed(&self, version: &str) -> bool {
        pomme_install::is_installed(&self.assets_dir, &self.versions_dir, version)
    }

    /// The downloaded Bedrock UI textures the Bedrock theme draws.
    pub fn bedrock_ui_dir(&self) -> PathBuf {
        pomme_install::bedrock::dir(&self.bedrock_assets_dir).join(pomme_install::bedrock::UI_DIR)
    }
}

#[cfg(test)]
mod tests {
    use super::DataDirs;

    #[test]
    fn each_minecraft_version_has_an_isolated_mod_directory() {
        let one = DataDirs::resolve(
            "1.20.1",
            Some("/tmp/pomme-assets"),
            Some("/tmp/pomme-versions"),
            Some("/tmp/pomme-profile"),
        );
        let two = DataDirs::resolve(
            "26.2",
            Some("/tmp/pomme-assets"),
            Some("/tmp/pomme-versions"),
            Some("/tmp/pomme-profile"),
        );
        assert_eq!(
            one.mods_dir,
            std::path::PathBuf::from("/tmp/pomme-profile/mods/1.20.1")
        );
        assert_eq!(
            two.mods_dir,
            std::path::PathBuf::from("/tmp/pomme-profile/mods/26.2")
        );
        assert_ne!(one.mods_dir, two.mods_dir);
    }
}
