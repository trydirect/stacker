//! Bake job — the internal BUILD step of the immutable-deploy model.
//!
//! A bake is: deploy a stack onto a throwaway build box (via the *existing*
//! deploy path), **health-gate** it, then **snapshot** it and record the
//! `image_id`. Only a green health-gate produces a published [`BakeRecord`];
//! that record's `image_id` is what user deploys later clone from
//! (`HetznerCloudConnector::create_server_from_image`).
//!
//! The gate/record decision is a pure function ([`evaluate_bake`]) so it is
//! unit-tested without infra; [`run_bake`] wires it to a real snapshot call.

use serde::{Deserialize, Serialize};

use crate::connectors::hetzner::{HetznerCloudConnector, HetznerSnapshotTarget};

/// A published, validated snapshot — the artifact user deploys clone from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BakeRecord {
    pub stack: String,
    pub version: String,
    pub provider: String,
    /// The baked snapshot's image id (Hetzner). Per-provider.
    pub image_id: i64,
    pub healthy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BakeError {
    /// Health-gate failed → snapshot is NOT published (this is the whole point:
    /// a bad stack never becomes a deployable artifact).
    #[error("bake rejected: build box unhealthy: {0}")]
    Unhealthy(String),
    /// The snapshot succeeded but Hetzner returned no image id.
    #[error("bake failed: snapshot returned no image_id")]
    NoImageId,
    #[error("bake backend error: {0}")]
    Backend(String),
}

/// Pure gate: only a healthy build box with a real `image_id` yields a record.
pub fn evaluate_bake(
    stack: &str,
    version: &str,
    provider: &str,
    healthy: bool,
    health_detail: &str,
    image_id: Option<i64>,
) -> Result<BakeRecord, BakeError> {
    if !healthy {
        return Err(BakeError::Unhealthy(health_detail.to_string()));
    }
    let image_id = image_id.ok_or(BakeError::NoImageId)?;
    Ok(BakeRecord {
        stack: stack.to_string(),
        version: version.to_string(),
        provider: provider.to_string(),
        image_id,
        healthy: true,
    })
}

/// Orchestrate a bake against a build box that already has the stack deployed:
/// verify `healthy` (probed by the caller), snapshot it, and record the id.
///
/// Note the ordering — we only snapshot when `healthy` is true, so an unhealthy
/// build box never produces a snapshot (no wasted image, no bad artifact).
pub async fn run_bake(
    connector: &dyn HetznerCloudConnector,
    token: &str,
    build_box: HetznerSnapshotTarget,
    stack: &str,
    version: &str,
    healthy: bool,
    health_detail: &str,
) -> Result<BakeRecord, BakeError> {
    if !healthy {
        return Err(BakeError::Unhealthy(health_detail.to_string()));
    }
    let description = format!("stacker bake {stack}:{version}");
    let snapshot = connector
        .create_server_snapshot(token, build_box, &description)
        .await
        .map_err(|e| BakeError::Backend(e.to_string()))?;

    evaluate_bake(stack, version, "hetzner", true, "", snapshot.image_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_with_image_id_yields_a_record() {
        let r = evaluate_bake("lamp", "v1", "hetzner", true, "", Some(888)).unwrap();
        assert_eq!(r.image_id, 888);
        assert_eq!(r.stack, "lamp");
        assert!(r.healthy);
    }

    #[test]
    fn unhealthy_is_rejected_before_publishing() {
        assert_eq!(
            evaluate_bake("lamp", "v1", "hetzner", false, "web: unhealthy", Some(888)),
            Err(BakeError::Unhealthy("web: unhealthy".into()))
        );
    }

    #[test]
    fn missing_image_id_is_an_error() {
        assert_eq!(
            evaluate_bake("lamp", "v1", "hetzner", true, "", None),
            Err(BakeError::NoImageId)
        );
    }

    // A mock connector proves run_bake's ordering without hitting Hetzner.
    struct MockOk;
    #[async_trait::async_trait]
    impl HetznerCloudConnector for MockOk {
        async fn create_server_snapshot(
            &self,
            _t: &str,
            _target: HetznerSnapshotTarget,
            _d: &str,
        ) -> Result<crate::connectors::hetzner::HetznerSnapshot, crate::connectors::ConnectorError>
        {
            Ok(crate::connectors::hetzner::HetznerSnapshot {
                action_id: 1,
                status: "success".into(),
                image_id: Some(999),
            })
        }
        async fn create_server_from_image(
            &self,
            _t: &str,
            _r: crate::connectors::hetzner::HetznerCreateServerRequest,
        ) -> Result<
            crate::connectors::hetzner::HetznerProvisionedServer,
            crate::connectors::ConnectorError,
        > {
            unreachable!()
        }
        async fn list_server_types(
            &self,
            _t: &str,
            _l: Option<&str>,
        ) -> Result<Vec<String>, crate::connectors::ConnectorError> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn run_bake_snapshots_only_when_healthy() {
        let target = HetznerSnapshotTarget {
            provider_server_id: Some(123),
            server_name: None,
            public_ip: None,
        };
        // Healthy → snapshots → record with the mock's image_id.
        let ok = run_bake(&MockOk, "tok", target.clone(), "lamp", "v1", true, "")
            .await
            .unwrap();
        assert_eq!(ok.image_id, 999);

        // Unhealthy → never calls the connector, rejected.
        let err = run_bake(&MockOk, "tok", target, "lamp", "v1", false, "db down")
            .await
            .unwrap_err();
        assert!(matches!(err, BakeError::Unhealthy(_)));
    }
}
