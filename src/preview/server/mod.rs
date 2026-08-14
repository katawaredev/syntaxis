use std::path::PathBuf;

use dioxus::{fullstack::HeaderMap, prelude::ServerFnError};
use futures_util::StreamExt;
use rand_core::{OsRng, RngCore};
use syntaxis_workspace::WorkspaceId;
use url::Url;

use super::{
    PreviewCandidate, PreviewConfig, PreviewLease, PreviewSession, PreviewShare, PreviewTarget,
};

mod gateway;
mod state;
mod target;

pub(crate) use gateway::dispatch;
use gateway::{gateway_url, preview_base_url, request_origin};
use state::{
    Lease, configs, leases, normalize_config, replace_workspace_lease, workspace_lease_mut,
};
use target::{
    CONNECT_TIMEOUT, MAX_DISCOVERED_LISTENERS, TARGET_PROBE_TIMEOUT, discover_workspace_listeners,
    http_client, probe_target, resolve_loopback_target, target_label, validate_target,
};

pub(crate) fn retire_workspace(workspace_id: &WorkspaceId) -> Result<(), ServerFnError> {
    configs()?
        .remove_workspace(&workspace_id.0)
        .map_err(internal)?;
    leases()?.retain(|_, lease| lease.workspace_id != *workspace_id);
    Ok(())
}

pub(super) async fn preview_config(workspace_id: String) -> Result<PreviewConfig, ServerFnError> {
    let workspace_id = WorkspaceId::new(workspace_id);
    crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let config = configs()?.workspace_config(&workspace_id.0);
    Ok(normalize_config(config))
}

pub(super) async fn preview_candidates(
    workspace_id: String,
) -> Result<Vec<PreviewCandidate>, ServerFnError> {
    let workspace_id = WorkspaceId::new(workspace_id);
    let workspace = crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let root = PathBuf::from(workspace.root);
    let listeners = tokio::task::spawn_blocking(move || discover_workspace_listeners(&root))
        .await
        .map_err(|_| internal("Could not inspect workspace preview processes."))?
        .map_err(internal)?;
    let client = http_client()?;
    let mut probes = futures_util::stream::FuturesUnordered::new();
    for candidate in listeners.into_iter().take(MAX_DISCOVERED_LISTENERS) {
        probes.push(async move {
            resolve_loopback_target(client, candidate.port, CONNECT_TIMEOUT)
                .await
                .map(|_| candidate)
        });
    }
    let mut candidates = Vec::new();
    while let Some(candidate) = probes.next().await {
        if let Some(candidate) = candidate {
            candidates.push(candidate);
        }
    }
    candidates.sort_by_key(|candidate| candidate.port);
    Ok(candidates)
}

pub(super) async fn create_preview_lease(
    workspace_id: String,
    target: PreviewTarget,
    headers: &HeaderMap,
) -> Result<PreviewLease, ServerFnError> {
    let workspace_id = WorkspaceId::new(workspace_id);
    crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let validated = validate_target(&target)?;
    let upstream = match &target {
        PreviewTarget::Loopback { port } => {
            resolve_loopback_target(http_client()?, *port, TARGET_PROBE_TIMEOUT)
                .await
                .ok_or_else(|| {
                    unavailable(format!(
                        "Could not connect to loopback port {port} from the Syntaxis runtime. \
                         Make sure the server is listening on 127.0.0.1, ::1, or a wildcard interface."
                    ))
                })?
        }
        PreviewTarget::Url { .. } => {
            probe_target(&validated).await?;
            validated
        }
    };
    let target_label = target_label(&upstream);
    let saved_target = match target {
        PreviewTarget::Loopback { port } => PreviewTarget::Loopback { port },
        PreviewTarget::Url { .. } => PreviewTarget::Url {
            url: upstream.as_str().to_owned(),
        },
    };

    let parent_origin = request_origin(headers)?;
    let gateway_base = preview_base_url(&parent_origin)?;
    let lease_id = random_hex(16);
    let public_url = gateway_url(&gateway_base, &format!("p-{lease_id}"))?;
    let public_authority = authority(&public_url)?;
    let public_origin = origin(&public_url)?;
    let secure = public_url.scheme() == "https";
    let lease = Lease {
        workspace_id,
        upstream,
        target_label,
        share_token: None,
        gateway_base,
        public_authority,
        public_origin,
        parent_origin,
        secure,
    };
    {
        let mut store = configs()?;
        let workspace_id = lease.workspace_id.0.clone();
        let previous = store.replace_workspace_config(
            workspace_id.clone(),
            PreviewConfig {
                target: Some(saved_target),
                port: None,
            },
        );
        if let Err(error) = store.save() {
            store.restore_workspace_config(workspace_id, previous);
            return Err(internal(error));
        }
    }
    let mut leases = leases()?;
    replace_workspace_lease(&mut leases, lease_id.clone(), lease);

    Ok(PreviewLease {
        id: lease_id,
        url: public_url.into(),
    })
}

pub(super) async fn create_preview_share(
    workspace_id: String,
    lease_id: String,
) -> Result<PreviewShare, ServerFnError> {
    let workspace_id = WorkspaceId::new(workspace_id);
    crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let mut leases = leases()?;
    let lease = workspace_lease_mut(&mut leases, &workspace_id, &lease_id)?;
    let token = random_hex(16);
    let url = gateway_url(&lease.gateway_base, &format!("s-{token}"))?.into();
    lease.share_token = Some(token);
    Ok(PreviewShare { url })
}

pub(super) async fn resume_preview_session(
    workspace_id: String,
    headers: &HeaderMap,
) -> Result<Option<PreviewSession>, ServerFnError> {
    let workspace_id = WorkspaceId::new(workspace_id);
    crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let parent_origin = request_origin(headers)?;
    let mut leases = leases()?;
    let Some((lease_id, lease)) = leases
        .iter_mut()
        .find(|(_, lease)| lease.workspace_id == workspace_id)
    else {
        return Ok(None);
    };
    refresh_preview_session(lease_id, lease, parent_origin).map(Some)
}

pub(super) async fn revoke_preview_share(
    workspace_id: String,
    lease_id: String,
) -> Result<(), ServerFnError> {
    let workspace_id = WorkspaceId::new(workspace_id);
    crate::workspace::api::server::workspace_by_id(&workspace_id).await?;
    let mut leases = leases()?;
    workspace_lease_mut(&mut leases, &workspace_id, &lease_id)?.share_token = None;
    Ok(())
}

fn refresh_preview_session(
    lease_id: &str,
    lease: &mut Lease,
    parent_origin: String,
) -> Result<PreviewSession, ServerFnError> {
    lease.parent_origin = parent_origin;
    let url = gateway_url(&lease.gateway_base, &format!("p-{lease_id}"))?.into();
    Ok(PreviewSession {
        lease: PreviewLease {
            id: lease_id.to_owned(),
            url,
        },
        share: active_preview_share(lease)?,
    })
}

fn active_preview_share(lease: &Lease) -> Result<Option<PreviewShare>, ServerFnError> {
    let Some(token) = lease.share_token.as_deref() else {
        return Ok(None);
    };
    Ok(Some(PreviewShare {
        url: gateway_url(&lease.gateway_base, &format!("s-{token}"))?.into(),
    }))
}

fn random_hex(length: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = vec![0_u8; length];
    OsRng.fill_bytes(&mut bytes);
    let mut output = String::with_capacity(length * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn authority(url: &Url) -> Result<String, ServerFnError> {
    let host = url
        .host()
        .ok_or_else(|| request_error("The preview origin has no hostname.", 500))?;
    let host = match host {
        url::Host::Domain(host) => host.to_owned(),
        url::Host::Ipv4(address) => address.to_string(),
        url::Host::Ipv6(address) => format!("[{address}]"),
    };
    Ok(url
        .port()
        .map_or(host.clone(), |port| format!("{host}:{port}")))
}

fn origin(url: &Url) -> Result<String, ServerFnError> {
    Ok(format!("{}://{}", url.scheme(), authority(url)?))
}

fn unavailable(message: impl Into<String>) -> ServerFnError {
    request_error(message, 503)
}

fn internal(message: impl Into<String>) -> ServerFnError {
    request_error(message, 500)
}

fn request_error(message: impl Into<String>, code: u16) -> ServerFnError {
    ServerFnError::ServerError {
        message: message.into(),
        code,
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER_ID: &str = "0123456789abcdef0123456789abcdef";
    const SHARE_ID: &str = "fedcba9876543210fedcba9876543210";

    fn test_lease(upstream: &str) -> Lease {
        Lease {
            workspace_id: WorkspaceId::new("workspace"),
            upstream: Url::parse(upstream).unwrap(),
            target_label: upstream.into(),
            share_token: None,
            gateway_base: Url::parse("https://preview.example.test/").unwrap(),
            public_authority: format!("p-{OWNER_ID}.preview.example.test"),
            public_origin: format!("https://p-{OWNER_ID}.preview.example.test"),
            parent_origin: "https://syntaxis.example.test".into(),
            secure: true,
        }
    }

    #[test]
    fn resume_reuses_owner_access_and_restores_the_share() {
        let mut lease = test_lease("http://127.0.0.1:5173/");
        lease.share_token = Some(SHARE_ID.into());

        let session = refresh_preview_session(
            OWNER_ID,
            &mut lease,
            "https://new-owner.example.test".into(),
        )
        .unwrap();

        assert_eq!(lease.parent_origin, "https://new-owner.example.test");
        assert_eq!(session.lease.id, OWNER_ID);
        assert_eq!(
            Url::parse(&session.lease.url).unwrap().host_str(),
            Some("p-0123456789abcdef0123456789abcdef.preview.example.test")
        );
        assert_eq!(
            session
                .share
                .as_ref()
                .and_then(|share| Url::parse(&share.url).ok())
                .and_then(|url| url.host_str().map(str::to_owned))
                .as_deref(),
            Some("s-fedcba9876543210fedcba9876543210.preview.example.test")
        );
    }
}
