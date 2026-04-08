use crate::db;
use crate::helpers::JsonResponse;
use actix_web::{get, web, HttpResponse, Responder, Result};
use sqlx::PgPool;

#[tracing::instrument(name = "List approved templates (public)", skip_all)]
#[get("")]
pub async fn list_handler(
    query: web::Query<TemplateListQuery>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let category = query.category.as_deref();
    let tag = query.tag.as_deref();
    let sort = query.sort.as_deref();

    db::marketplace::list_approved(pg_pool.get_ref(), category, tag, sort)
        .await
        .map_err(|err| {
            JsonResponse::<Vec<crate::models::StackTemplate>>::build().internal_server_error(err)
        })
        .map(|templates| JsonResponse::build().set_list(templates).ok("OK"))
}

#[tracing::instrument(name = "Generate install script", skip_all)]
#[get("/install/{purchase_token}")]
pub async fn install_script_handler(path: web::Path<String>) -> Result<HttpResponse> {
    let purchase_token = path.into_inner();
    let script = generate_install_script(&purchase_token);

    Ok(HttpResponse::Ok()
        .content_type("text/x-shellscript")
        .insert_header(("Content-Disposition", "inline; filename=\"install.sh\""))
        .body(script))
}

fn generate_install_script(purchase_token: &str) -> String {
    let stacker_url = std::env::var("STACKER_PUBLIC_URL")
        .unwrap_or_else(|_| "https://stacker.try.direct".to_string());

    format!(
        r#"#!/bin/sh
set -e

PURCHASE_TOKEN="{purchase_token}"
STACKER_URL="{stacker_url}"

echo "============================================"
echo "  TryDirect Marketplace Stack Installer"
echo "============================================"
echo ""

# 1. Install Stacker CLI
echo "[1/4] Installing Stacker CLI..."
if ! command -v stacker >/dev/null 2>&1; then
    curl -sSfL "$STACKER_URL/releases/stacker-cli/install.sh" | sh
else
    echo "  Stacker CLI already installed."
fi

# 2. Install Status Panel agent
echo "[2/4] Installing Status Panel agent..."
if ! command -v status-panel >/dev/null 2>&1; then
    curl -sSfL "$STACKER_URL/releases/status-panel/install.sh" | sh
else
    echo "  Status Panel already installed."
fi

# 3. Download stack archive
echo "[3/4] Downloading stack..."
STACK_DIR="/opt/stacker/marketplace/$PURCHASE_TOKEN"
mkdir -p "$STACK_DIR"
curl -sSfL "$STACKER_URL/api/v1/marketplace/download/$PURCHASE_TOKEN" -o "$STACK_DIR/stack.tar.gz"
cd "$STACK_DIR"
tar xzf stack.tar.gz
rm stack.tar.gz

# 4. Register agent and deploy
echo "[4/4] Registering agent and deploying stack..."
STACK_ID=$(cat "$STACK_DIR/stack.json" 2>/dev/null | grep -o '"stack_id"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | cut -d'"' -f4)
if [ -z "$STACK_ID" ]; then
    STACK_ID="unknown"
fi

status-panel register --token "$PURCHASE_TOKEN" --stack-id "$STACK_ID" --server "$STACKER_URL"

echo ""
echo "Deploying stack..."
cd "$STACK_DIR"
stacker deploy --target local

echo ""
echo "============================================"
echo "  Installation complete!"
echo "============================================"
echo ""
echo "Status Panel is running. Access it at:"
echo "  http://$(hostname -I | awk '{{print $1}}'):5000"
echo ""
echo "Your deployment is linked to your TryDirect dashboard."
echo ""
"#,
        purchase_token = purchase_token,
        stacker_url = stacker_url
    )
}

#[tracing::instrument(name = "Download stack archive", skip_all)]
#[get("/download/{purchase_token}")]
pub async fn download_stack_handler(
    path: web::Path<String>,
    _pg_pool: web::Data<PgPool>,
) -> Result<HttpResponse> {
    let purchase_token = path.into_inner();

    // TODO: Call User Service POST /marketplace/purchase-token/validate
    // to verify token and get stack_id, then locate and serve the archive.
    tracing::info!(
        "Stack download requested for purchase_token={}",
        purchase_token
    );

    Ok(HttpResponse::Ok()
        .content_type("application/gzip")
        .insert_header((
            "Content-Disposition",
            format!(
                "attachment; filename=\"stack-{}.tar.gz\"",
                purchase_token
            ),
        ))
        .body("stack archive placeholder"))
}

#[derive(Debug, serde::Deserialize)]
pub struct TemplateListQuery {
    pub category: Option<String>,
    pub tag: Option<String>,
    pub sort: Option<String>, // recent|popular|rating
}

#[tracing::instrument(name = "Get template by slug (public)", skip_all)]
#[get("/{slug}")]
pub async fn detail_handler(
    path: web::Path<(String,)>,
    pg_pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    let slug = path.into_inner().0;

    match db::marketplace::get_by_slug_with_latest(pg_pool.get_ref(), &slug).await {
        Ok((template, version)) => {
            let mut payload = serde_json::json!({
                "template": template,
            });
            if let Some(ver) = version {
                payload["latest_version"] = serde_json::to_value(ver).unwrap();
            }
            Ok(JsonResponse::build().set_item(Some(payload)).ok("OK"))
        }
        Err(err) => Err(JsonResponse::<serde_json::Value>::build().not_found(err)),
    }
}
