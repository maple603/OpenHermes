//! System prompt builder.

/// Build the system prompt for the agent
pub fn build_system_prompt(
    platform: &str,
    memory_context: Option<&str>,
    skills_context: Option<&str>,
    _context_files: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Agent identity
    parts.push(DEFAULT_AGENT_IDENTITY.to_string());

    // Platform hints
    if let Some(platform_hint) = get_platform_hint(platform) {
        parts.push(platform_hint.to_string());
    }

    // Memory context
    if let Some(mem) = memory_context {
        if !mem.trim().is_empty() {
            parts.push(wrap_memory_context(mem));
        }
    }

    // Skills context
    if let Some(skills) = skills_context {
        if !skills.trim().is_empty() {
            parts.push(skills.to_string());
        }
    }

    parts.join("\n\n")
}

/// Default agent identity
pub const DEFAULT_AGENT_IDENTITY: &str = r#"You are Hermes, a helpful AI assistant built by Nous Research.
You are capable of executing tools and can interact with the user's environment.
Be direct, honest, and helpful in your responses."#;

/// Platform-specific hints
pub fn get_platform_hint(platform: &str) -> Option<&'static str> {
    PLATFORM_HINTS.iter()
        .find(|(p, _)| *p == platform)
        .map(|(_, hint)| *hint)
}

/// Platform-specific hints as array
pub const PLATFORM_HINTS: &[(&str, &str)] = &[
    ("cli", "You are running in a terminal CLI interface."),
    ("telegram", "You are running in a Telegram bot."),
    ("discord", "You are running in a Discord bot."),
];

/// Wrap memory context in XML tags
pub fn wrap_memory_context(context: &str) -> String {
    format!(
        "<memory-context>\n[System note: The following is recalled memory context, NOT new user input.]\n\n{}\n</memory-context>",
        context
    )
}
