use super::context::ObfuscationContext;
use rand::Rng;

#[derive(Debug, Clone, Copy)]
pub enum NameKind {
    Variable,
    List,
    Broadcast,
    Procedure,
    Bait,
    Sprite,
    Checksum,
}

pub fn generate_name(kind: NameKind, ctx: &mut ObfuscationContext) -> String {
    loop {
        let candidate = match kind {
            NameKind::Variable => format!("_0x{:04X}", ctx.rng.gen_range(0x1000u32..=0xFFFF)),
            NameKind::List => format!("_v_{}", ctx.rng.gen_range(100000u32..999999u32)),
            NameKind::Broadcast => ambiguous_pattern(ctx, "O0o"),
            NameKind::Procedure => format!("proc_{}", ambiguous_pattern(ctx, "lI1")),
            NameKind::Bait => format!("_v_{}", ctx.rng.gen_range(1000u32..9999u32)),
            NameKind::Sprite => format!("sprite_{}", ambiguous_pattern(ctx, "O0l")),
            NameKind::Checksum => format!("_chk_{:04X}", ctx.rng.gen_range(0x1000u32..=0xFFFF)),
        };
        let lowered = candidate.to_ascii_lowercase();
        if ctx.used_names.insert(lowered) {
            return candidate;
        }
    }
}

pub fn generate_identifier(prefix: &str, ctx: &mut ObfuscationContext) -> String {
    loop {
        let candidate = format!(
            "{}_{:08x}{:04x}",
            prefix,
            ctx.rng.gen_range(0u32..=u32::MAX),
            ctx.rng.gen_range(0u16..=u16::MAX)
        );
        if ctx.used_ids.insert(candidate.clone()) {
            return candidate;
        }
    }
}

fn ambiguous_pattern(ctx: &mut ObfuscationContext, alphabet: &str) -> String {
    let chars = alphabet.chars().collect::<Vec<_>>();
    let len = ctx.rng.gen_range(6usize..11usize);
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        let idx = ctx.rng.gen_range(0..chars.len());
        out.push(chars[idx]);
    }
    out
}

pub fn ensure_unique_name(base: &str, ctx: &mut ObfuscationContext) -> String {
    let trimmed = if base.trim().is_empty() {
        "bait"
    } else {
        base.trim()
    };
    let lowered = trimmed.to_ascii_lowercase();
    if ctx.used_names.insert(lowered) {
        return trimmed.to_string();
    }

    let mut counter = 2usize;
    loop {
        let candidate = format!("{}_{}", trimmed, counter);
        if ctx.used_names.insert(candidate.to_ascii_lowercase()) {
            return candidate;
        }
        counter += 1;
    }
}

pub fn rewrite_proccode_name(original: &str, new_name: &str) -> String {
    let tokens = original.split_whitespace().collect::<Vec<_>>();
    let first_placeholder = tokens
        .iter()
        .position(|token| token.starts_with('%'))
        .unwrap_or(tokens.len());
    if first_placeholder >= tokens.len() {
        return new_name.to_string();
    }
    format!("{} {}", new_name, tokens[first_placeholder..].join(" "))
}

pub fn clicker_like_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "coin",
        "coins",
        "money",
        "cash",
        "gem",
        "gems",
        "diamond",
        "diamonds",
        "rebirth",
        "rebirths",
        "prestige",
        "click",
        "cpc",
        "cps",
        "per click",
        "per second",
        "upgrade",
        "upgrades",
        "save",
        "save code",
        "level",
        "xp",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}
