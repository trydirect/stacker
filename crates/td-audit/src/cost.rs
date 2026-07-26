//! Multi-Cloud Cost & Sizing Estimator (#4).
//!
//! Sizes a stack from its compose (declared `deploy.resources`, with defaults
//! for undeclared services + host overhead), then finds the cheapest instance
//! per provider that fits. Pricing is injected via [`PricingSource`] so the
//! crate stays pure; [`DefaultPricing`] is a maintained static snapshot the
//! server can use directly.

use serde::{Deserialize, Serialize};

use crate::compose::parse_compose;

/// Per-service assumptions when a service declares no resource limits.
const DEFAULT_CPUS_PER_SERVICE: f64 = 0.25;
const DEFAULT_MEM_MB_PER_SERVICE: u64 = 256;
/// Fixed host overhead (OS + docker) added on top of the workload.
const HOST_OVERHEAD_CPUS: f64 = 0.5;
const HOST_OVERHEAD_MEM_MB: u64 = 512;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceOption {
    pub provider: String,
    pub name: String,
    pub vcpus: f64,
    pub memory_mb: u64,
    pub monthly_usd: f64,
}

/// Source of instance/pricing data. Injected so the engine is testable without
/// network and the server can back it with a refreshed table.
pub trait PricingSource {
    fn instances(&self) -> Vec<InstanceOption>;
    /// e.g. "2026-07" — surfaced to the user as "prices as of …".
    fn as_of(&self) -> String;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackSizing {
    pub service_count: usize,
    pub total_cpus: f64,
    pub total_memory_mb: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderQuote {
    pub provider: String,
    pub instance: String,
    pub vcpus: f64,
    pub memory_mb: u64,
    pub monthly_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    pub sizing: StackSizing,
    /// Cheapest fitting instance per provider, ascending by price.
    pub quotes: Vec<ProviderQuote>,
    pub cheapest: Option<ProviderQuote>,
    pub priced_as_of: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Size a stack from its compose file.
pub fn size_stack(yaml: &str) -> Result<StackSizing, String> {
    let model = parse_compose(yaml).map_err(|e| e.to_string())?;
    let mut total_cpus = HOST_OVERHEAD_CPUS;
    let mut total_memory_mb = HOST_OVERHEAD_MEM_MB;
    for svc in &model.services {
        total_cpus += svc.cpus.unwrap_or(DEFAULT_CPUS_PER_SERVICE);
        total_memory_mb += svc.memory_mb.unwrap_or(DEFAULT_MEM_MB_PER_SERVICE);
    }
    Ok(StackSizing {
        service_count: model.services.len(),
        total_cpus: (total_cpus * 100.0).round() / 100.0,
        total_memory_mb,
    })
}

/// Cheapest instance per provider that fits the sizing.
pub fn cheapest_per_provider(sizing: &StackSizing, pricing: &dyn PricingSource) -> Vec<ProviderQuote> {
    use std::collections::BTreeMap;
    let mut best: BTreeMap<String, ProviderQuote> = BTreeMap::new();
    for opt in pricing.instances() {
        if opt.vcpus + 1e-9 < sizing.total_cpus || opt.memory_mb < sizing.total_memory_mb {
            continue; // does not fit
        }
        let quote = ProviderQuote {
            provider: opt.provider.clone(),
            instance: opt.name.clone(),
            vcpus: opt.vcpus,
            memory_mb: opt.memory_mb,
            monthly_usd: opt.monthly_usd,
        };
        best.entry(opt.provider.clone())
            .and_modify(|q| {
                if quote.monthly_usd < q.monthly_usd {
                    *q = quote.clone();
                }
            })
            .or_insert(quote);
    }
    let mut quotes: Vec<ProviderQuote> = best.into_values().collect();
    quotes.sort_by(|a, b| a.monthly_usd.partial_cmp(&b.monthly_usd).unwrap());
    quotes
}

/// Full estimate for a compose file against a pricing source.
pub fn estimate_cost(yaml: &str, pricing: &dyn PricingSource) -> CostEstimate {
    match size_stack(yaml) {
        Ok(sizing) => {
            let quotes = cheapest_per_provider(&sizing, pricing);
            let cheapest = quotes.first().cloned();
            CostEstimate {
                sizing,
                quotes,
                cheapest,
                priced_as_of: pricing.as_of(),
                error: None,
            }
        }
        Err(e) => CostEstimate {
            sizing: StackSizing {
                service_count: 0,
                total_cpus: 0.0,
                total_memory_mb: 0,
            },
            quotes: vec![],
            cheapest: None,
            priced_as_of: pricing.as_of(),
            error: Some(e),
        },
    }
}

/// Maintained static pricing snapshot (approximate; refresh periodically).
pub struct DefaultPricing;

impl PricingSource for DefaultPricing {
    fn as_of(&self) -> String {
        "2026-07".to_string()
    }
    fn instances(&self) -> Vec<InstanceOption> {
        let i = |provider: &str, name: &str, vcpus: f64, memory_mb: u64, monthly_usd: f64| {
            InstanceOption {
                provider: provider.into(),
                name: name.into(),
                vcpus,
                memory_mb,
                monthly_usd,
            }
        };
        vec![
            // Hetzner Cloud (shared vCPU)
            i("hetzner", "cpx11", 2.0, 2048, 4.35),
            i("hetzner", "cpx21", 3.0, 4096, 7.55),
            i("hetzner", "cpx31", 4.0, 8192, 14.60),
            // DigitalOcean
            i("digitalocean", "s-1vcpu-1gb", 1.0, 1024, 6.0),
            i("digitalocean", "s-1vcpu-2gb", 1.0, 2048, 12.0),
            i("digitalocean", "s-2vcpu-4gb", 2.0, 4096, 24.0),
            // Linode
            i("linode", "nanode-1gb", 1.0, 1024, 5.0),
            i("linode", "linode-2gb", 1.0, 2048, 12.0),
            i("linode", "linode-4gb", 2.0, 4096, 24.0),
            // Vultr
            i("vultr", "vc2-1c-1gb", 1.0, 1024, 5.0),
            i("vultr", "vc2-1c-2gb", 1.0, 2048, 10.0),
            i("vultr", "vc2-2c-4gb", 2.0, 4096, 20.0),
            // AWS Lightsail (rough)
            i("aws", "lightsail-2gb", 2.0, 2048, 12.0),
            i("aws", "lightsail-4gb", 2.0, 4096, 24.0),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPricing;
    impl PricingSource for TestPricing {
        fn as_of(&self) -> String {
            "test".into()
        }
        fn instances(&self) -> Vec<InstanceOption> {
            vec![
                InstanceOption { provider: "hetzner".into(), name: "small".into(), vcpus: 2.0, memory_mb: 2048, monthly_usd: 5.0 },
                InstanceOption { provider: "hetzner".into(), name: "big".into(), vcpus: 4.0, memory_mb: 8192, monthly_usd: 15.0 },
                InstanceOption { provider: "aws".into(), name: "pricey".into(), vcpus: 2.0, memory_mb: 2048, monthly_usd: 20.0 },
            ]
        }
    }

    const LAMP: &str = r#"
services:
  web:
    image: nginx:1.27-alpine
    deploy: { resources: { limits: { cpus: "0.5", memory: 256M } } }
  db:
    image: postgres:16
    deploy: { resources: { limits: { cpus: "0.5", memory: 512M } } }
"#;

    #[test]
    fn sizes_declared_plus_overhead() {
        let s = size_stack(LAMP).unwrap();
        assert_eq!(s.service_count, 2);
        // 0.5 host + 0.5 + 0.5 = 1.5 cpus; 512 host + 256 + 512 = 1280 MB
        assert_eq!(s.total_cpus, 1.5);
        assert_eq!(s.total_memory_mb, 1280);
    }

    #[test]
    fn undeclared_services_use_defaults() {
        let yaml = "services:\n  a: { image: x }\n  b: { image: y }\n";
        let s = size_stack(yaml).unwrap();
        // 0.5 host + 0.25*2 = 1.0 ; 512 host + 256*2 = 1024
        assert_eq!(s.total_cpus, 1.0);
        assert_eq!(s.total_memory_mb, 1024);
    }

    #[test]
    fn picks_cheapest_fitting_instance_per_provider() {
        let est = estimate_cost(LAMP, &TestPricing);
        // Both hetzner options fit; the cheaper "small" wins for hetzner.
        let hetzner = est.quotes.iter().find(|q| q.provider == "hetzner").unwrap();
        assert_eq!(hetzner.instance, "small");
        // Cheapest overall is hetzner/small at 5.0.
        assert_eq!(est.cheapest.as_ref().unwrap().provider, "hetzner");
        assert_eq!(est.cheapest.as_ref().unwrap().monthly_usd, 5.0);
        // Quotes are ascending by price.
        assert!(est.quotes.windows(2).all(|w| w[0].monthly_usd <= w[1].monthly_usd));
    }

    #[test]
    fn default_pricing_covers_five_providers() {
        let providers: std::collections::BTreeSet<_> =
            DefaultPricing.instances().into_iter().map(|i| i.provider).collect();
        for p in ["hetzner", "digitalocean", "linode", "vultr", "aws"] {
            assert!(providers.contains(p), "missing provider {p}");
        }
    }
}
