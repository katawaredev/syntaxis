//! Browser provider keys are deliberately held only in memory.

use super::*;
use syntaxis_module_ai::{
    AiAuthFlow, AiAuthPrompt, AiProviderAccount, AiProviderAuthKind, AiProviderAuthMethod,
    AiProviderAuthPort,
};

const PROVIDERS: [(&str, &str); 8] = [
    ("openai", "OpenAI"),
    ("anthropic", "Anthropic"),
    ("google", "Google"),
    ("mistral", "Mistral"),
    ("groq", "Groq"),
    ("openrouter", "OpenRouter"),
    ("xai", "xAI"),
    ("custom", "Custom OpenAI-compatible"),
];

#[async_trait(?Send)]
impl AiProviderAuthPort for BrowserAiAdapter {
    async fn list(&self, _workspace: &WorkspaceRecord) -> Result<Vec<AiProviderAccount>, AppError> {
        let state = self.lock();
        Ok(PROVIDERS
            .iter()
            .map(|(id, name)| {
                let configured = state.credentials.contains_key(*id);
                AiProviderAccount {
                    id: (*id).into(),
                    name: (*name).into(),
                    configured,
                    can_logout: configured,
                    status: if configured {
                        "API key held in memory for this tab."
                    } else {
                        "Enter your API key. OAuth requires the server runtime."
                    }
                    .into(),
                    methods: vec![AiProviderAuthMethod {
                        kind: AiProviderAuthKind::ApiKey,
                        label: "API key".into(),
                    }],
                }
            })
            .collect())
    }

    async fn start(
        &self,
        _workspace: &WorkspaceRecord,
        provider_id: &str,
        kind: AiProviderAuthKind,
    ) -> Result<AiAuthFlow, AppError> {
        if kind != AiProviderAuthKind::ApiKey || !PROVIDERS.iter().any(|(id, _)| *id == provider_id)
        {
            return Err(ai_error(
                AppErrorCode::InvalidInput,
                "This browser provider supports API keys only.",
            ));
        }
        let flow = AiAuthFlow {
            id: self.id("auth"),
            provider_id: provider_id.into(),
            prompt: Some(AiAuthPrompt {
                id: 1,
                kind: "secret".into(),
                message: "API key (cleared when this tab closes)".into(),
                placeholder: "API key".into(),
                options: Vec::new(),
            }),
            events: Vec::new(),
            complete: false,
            error: None,
        };
        let mut state = self.lock();
        state.auth_flows.clear();
        state.auth_flows.insert(flow.id.clone(), flow.clone());
        Ok(flow)
    }

    async fn status(
        &self,
        _workspace: &WorkspaceRecord,
        flow_id: &str,
    ) -> Result<AiAuthFlow, AppError> {
        self.lock()
            .auth_flows
            .get(flow_id)
            .cloned()
            .ok_or_else(|| ai_error(AppErrorCode::NotFound, "The credential prompt has expired."))
    }

    async fn respond(
        &self,
        _workspace: &WorkspaceRecord,
        flow_id: &str,
        prompt_id: u64,
        answer: &str,
    ) -> Result<(), AppError> {
        let answer = answer.trim();
        if answer.is_empty() || answer.len() > MAX_CREDENTIAL_BYTES {
            return Err(ai_error(
                AppErrorCode::InvalidInput,
                "Enter an API key of at most 16 KiB.",
            ));
        }
        if !answer.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err(ai_error(
                AppErrorCode::InvalidInput,
                "The API key contains whitespace or unsupported characters. Paste only the provider API key.",
            ));
        }
        let mut state = self.lock();
        let flow = state.auth_flows.get_mut(flow_id).ok_or_else(|| {
            ai_error(AppErrorCode::NotFound, "The credential prompt has expired.")
        })?;
        if flow.complete || flow.prompt.as_ref().map(|prompt| prompt.id) != Some(prompt_id) {
            return Err(ai_error(
                AppErrorCode::InvalidInput,
                "The credential prompt is no longer active.",
            ));
        }
        flow.complete = true;
        flow.prompt = None;
        let provider = flow.provider_id.clone();
        state.credentials.insert(provider, answer.to_owned());
        Ok(())
    }

    async fn cancel(&self, _workspace: &WorkspaceRecord, flow_id: &str) -> Result<(), AppError> {
        self.lock().auth_flows.remove(flow_id);
        Ok(())
    }

    async fn logout(
        &self,
        _workspace: &WorkspaceRecord,
        provider_id: &str,
    ) -> Result<(), AppError> {
        let mut state = self.lock();
        state.credentials.remove(provider_id);
        state
            .auth_flows
            .retain(|_, flow| flow.provider_id != provider_id);
        Ok(())
    }
}
