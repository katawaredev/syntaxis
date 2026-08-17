use dioxus::prelude::*;
use dioxus_devicons::devicons::monochrome;

use crate::icons::{AppIcon, Icon};

/// Displays the closest available Devicons mark for an AI provider.
///
/// Provider identifiers come from the agent runtime and are not a closed set, so unknown
/// providers deliberately fall back to the generic bot icon.
#[component]
pub fn ProviderIcon(provider: String, #[props(default = 16)] size: u32) -> Element {
    let provider = provider.to_ascii_lowercase();

    if provider.contains("azure") {
        rsx! {
            monochrome::MicrosoftAzure { size }
        }
    } else if provider.contains("bedrock") || provider.contains("amazon") {
        rsx! {
            monochrome::AwsBedrockIcon { size }
        }
    } else if provider.contains("github") || provider.contains("copilot") {
        rsx! {
            monochrome::GithubCopilot { size }
        }
    } else if provider.contains("anthropic") || provider.contains("claude") {
        rsx! {
            monochrome::AnthropicIcon { size }
        }
    } else if provider.contains("openai") || provider.contains("codex") {
        rsx! {
            monochrome::OpenaiIcon { size }
        }
    } else if provider.contains("google") || provider.contains("gemini") {
        rsx! {
            monochrome::GoogleGeminiIcon { size }
        }
    } else if provider.contains("mistral") {
        rsx! {
            monochrome::MistralAiIcon { size }
        }
    } else if provider.contains("cohere") {
        rsx! {
            monochrome::CohereIcon { size }
        }
    } else if provider.contains("perplexity") {
        rsx! {
            monochrome::PerplexityIcon { size }
        }
    } else if provider.contains("deepseek") {
        rsx! {
            monochrome::Deepseek { size }
        }
    } else if provider.contains("hugging") {
        rsx! {
            monochrome::HuggingFaceIcon { size }
        }
    } else if provider.contains("cloudflare") {
        rsx! {
            monochrome::CloudflareIcon { size }
        }
    } else if provider.contains("together") {
        rsx! {
            monochrome::Together { size }
        }
    } else if provider.contains("ollama") {
        rsx! {
            monochrome::OllamaIcon { size }
        }
    } else if provider.contains("groq") {
        rsx! {
            monochrome::Groq { size }
        }
    } else if provider.contains("xai") || provider.contains("grok") {
        rsx! {
            monochrome::Xai { size }
        }
    } else {
        rsx! {
            Icon { icon: AppIcon::Bot, size }
        }
    }
}
