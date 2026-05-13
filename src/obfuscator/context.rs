use super::config::ObfuscationLevel;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::collections::{HashMap, HashSet};

pub struct ObfuscationContext {
    pub rng: ChaCha8Rng,
    pub seed: u64,
    pub level: ObfuscationLevel,
    pub used_names: HashSet<String>,
    pub used_ids: HashSet<String>,
    pub protected_variable_names: HashSet<String>,
    pub warnings: Vec<String>,
    pub metadata: Vec<ProtectedVariableMetadata>,
    pub applied_passes: Vec<String>,
    pub original_variable_names: HashMap<String, String>,
    pub bait_name_candidates: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProtectedVariableMetadata {
    pub target_name: String,
    pub original_name: String,
    pub real_name: String,
    pub real_id: String,
    pub fake_id: String,
    pub checksum_name: String,
    pub checksum_id: String,
    pub checksum_secret: i64,
}

impl ObfuscationContext {
    pub fn new(
        seed: u64,
        level: ObfuscationLevel,
        used_names: HashSet<String>,
        used_ids: HashSet<String>,
        protected_variable_names: HashSet<String>,
    ) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
            seed,
            level,
            used_names,
            used_ids,
            protected_variable_names,
            warnings: Vec::new(),
            metadata: Vec::new(),
            applied_passes: Vec::new(),
            original_variable_names: HashMap::new(),
            bait_name_candidates: Vec::new(),
        }
    }

    pub fn note_pass(&mut self, name: &str) {
        self.applied_passes.push(name.to_string());
    }

    pub fn push_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    pub fn is_protected_variable_name(&self, name: &str) -> bool {
        self.protected_variable_names
            .contains(&name.trim().to_ascii_lowercase())
    }

    pub fn push_bait_name_candidate(&mut self, name: impl Into<String>) {
        self.bait_name_candidates.push(name.into());
    }
}
