#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JvmArgs {
    preset: String,
    flags: Vec<String>,
    custom: Vec<String>,
    dirty: bool,
}

impl Default for JvmArgs {
    fn default() -> Self {
        Self {
            preset: "Custom".to_string(),
            flags: Vec::new(),
            custom: Vec::new(),
            dirty: false,
        }
    }
}

impl JvmArgs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_preset(mut self, preset: &str) -> Self {
        self.preset = preset.to_string();
        self.flags = Self::preset_flags(preset);
        self.custom.clear();
        self.dirty = false;
        self
    }

    pub fn set_custom(&mut self, custom: Vec<String>) {
        self.custom = custom;
        self.preset = "Custom".to_string();
        self.flags.clear();
        self.dirty = true;
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn switch_preset(&mut self, preset: &str) -> Option<String> {
        if self.dirty && !self.custom.is_empty() {
            return Some(format!(
                "Unsaved custom JVM arguments will be discarded when switching to {preset}."
            ));
        }

        self.preset = preset.to_string();
        self.flags = Self::preset_flags(preset);
        self.custom.clear();
        self.dirty = false;
        None
    }

    pub fn command_line(&self) -> Vec<String> {
        if !self.custom.is_empty() {
            return self.custom.clone();
        }

        if !self.flags.is_empty() {
            return self.flags.clone();
        }

        Vec::new()
    }

    pub fn flags(&self) -> &[String] {
        &self.flags
    }

    fn preset_flags(preset: &str) -> Vec<String> {
        match preset {
            "Performance" => vec![
                "-XX:+UseG1GC".to_string(),
                "-XX:MaxGCPauseMillis=200".to_string(),
                "-XX:+UnlockExperimentalVMOptions".to_string(),
                "-XX:+AlwaysPreTouch".to_string(),
                "-Dusing.aikars.flags=https://mcflags.emc.gs".to_string(),
                "-Daikars.new.flags=true".to_string(),
            ],
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn performance_preset_fills_aikar_flags() {
        let preset = JvmArgs::new().with_preset("Performance");

        assert!(preset.flags().iter().any(|flag| flag == "-XX:+UseG1GC"));
        assert!(preset
            .flags()
            .iter()
            .any(|flag| flag == "-XX:MaxGCPauseMillis=200"));
    }

    #[test]
    fn custom_arguments_are_preserved_verbatim() {
        let mut args = JvmArgs::new();
        args.set_custom(vec!["-Xms1G".to_string(), "-Dfoo=bar".to_string()]);

        assert_eq!(args.command_line(), vec!["-Xms1G", "-Dfoo=bar"]);
    }

    #[test]
    fn switching_presets_prompts_when_custom_edits_are_unsaved() {
        let mut args = JvmArgs::new();
        args.set_custom(vec!["-Xms1G".to_string()]);
        args.mark_dirty();

        assert!(args.switch_preset("Performance").is_some());
    }
}
