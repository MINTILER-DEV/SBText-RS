use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ObfuscationLevel {
    Low,
    Medium,
    High,
    Insane,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ObfuscationPreset {
    Clicker,
}

#[derive(Debug, Clone)]
pub struct ObfuscationConfig {
    pub level: ObfuscationLevel,
    pub rename: bool,
    pub wrap_procedures: bool,
    pub flatten_control_flow: bool,
    pub randomize_ids: bool,
    pub scramble_layout: bool,
    pub inject_junk: bool,
    pub protect_vars: Vec<String>,
    pub preset: Option<ObfuscationPreset>,
    pub seed: Option<u64>,
}

impl Default for ObfuscationConfig {
    fn default() -> Self {
        Self {
            level: ObfuscationLevel::Medium,
            rename: false,
            wrap_procedures: false,
            flatten_control_flow: false,
            randomize_ids: false,
            scramble_layout: false,
            inject_junk: false,
            protect_vars: Vec::new(),
            preset: None,
            seed: None,
        }
    }
}

impl ObfuscationLevel {
    pub fn defaults(self) -> (bool, bool, bool, bool, bool, bool) {
        match self {
            Self::Low => (true, false, false, true, false, false),
            Self::Medium => (true, false, false, true, true, true),
            Self::High => (true, true, true, true, true, true),
            Self::Insane => (true, true, true, true, true, true),
        }
    }
}
