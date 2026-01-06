use std::sync::Arc;

use crate::connectors::{DockerHubConnector, NamespaceSummary, RepositorySummary, TagSummary};
use crate::helpers::JsonResponse;
use actix_web::{get, web, Error, Responder};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct AutocompleteQuery {
    #[serde(default)]
    pub q: Option<String>,
}

#[derive(Deserialize)]
pub struct NamespacePath {
    pub namespace: String,
}

#[derive(Deserialize)]
pub struct RepositoryPath {
    pub namespace: String,
    pub repository: String,
}

#[tracing::instrument(
    name = "dockerhub_search_namespaces",
    skip(connector),
    fields(query = query.q.as_deref().unwrap_or_default())
)]
#[get("/namespaces")]
pub async fn search_namespaces(
    connector: web::Data<Arc<dyn DockerHubConnector>>,
    query: web::Query<AutocompleteQuery>,
) -> Result<impl Responder, Error> {
    let term = query.q.as_deref().unwrap_or_default();
    connector
        .search_namespaces(term)
        .await
        .map(|namespaces| JsonResponse::<NamespaceSummary>::build().set_list(namespaces).ok("OK"))
        .map_err(Error::from)
}

#[tracing::instrument(
    name = "dockerhub_list_repositories",
    skip(connector),
    fields(namespace = %path.namespace, query = query.q.as_deref().unwrap_or_default())
)]
#[get("/{namespace}/repositories")]
pub async fn list_repositories(
    connector: web::Data<Arc<dyn DockerHubConnector>>,
    path: web::Path<NamespacePath>,
    query: web::Query<AutocompleteQuery>,
) -> Result<impl Responder, Error> {
    let params = path.into_inner();
    connector
        .list_repositories(&params.namespace, query.q.as_deref())
        .await
        .map(|repos| JsonResponse::<RepositorySummary>::build().set_list(repos).ok("OK"))
        .map_err(Error::from)
}

#[tracing::instrument(
    name = "dockerhub_list_tags",
    skip(connector),
    fields(namespace = %path.namespace, repository = %path.repository, query = query.q.as_deref().unwrap_or_default())
)]
#[get("/{namespace}/repositories/{repository}/tags")]
pub async fn list_tags(
    connector: web::Data<Arc<dyn DockerHubConnector>>,
    path: web::Path<RepositoryPath>,
    query: web::Query<AutocompleteQuery>,
) -> Result<impl Responder, Error> {
    let params = path.into_inner();
    connector
        .list_tags(&params.namespace, &params.repository, query.q.as_deref())
        .await
        .map(|tags| JsonResponse::<TagSummary>::build().set_list(tags).ok("OK"))
        .map_err(Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::dockerhub_cservice::mock::MockDockerHubConnector;
    use actix_web::{http::StatusCode, test, App};

    #[actix_web::test]
    async fn dockerhub_namespaces_endpoint_returns_data() {
        let connector: Arc<dyn DockerHubConnector> = Arc::new(MockDockerHubConnector::default());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(connector))
                .service(search_namespaces),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/namespaces?q=stacker")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["message"], "OK");
        assert!(body["list"].is_array());
    }

    #[actix_web::test]
    async fn dockerhub_repositories_endpoint_returns_data() {
        let connector: Arc<dyn DockerHubConnector> = Arc::new(MockDockerHubConnector::default());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(connector))
                .service(list_repositories),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/example/repositories?q=stacker")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["message"], "OK");
        assert!(body["list"].as_array().unwrap().len() >= 1);
    }

    #[actix_web::test]
    async fn dockerhub_tags_endpoint_returns_data() {
        let connector: Arc<dyn DockerHubConnector> = Arc::new(MockDockerHubConnector::default());
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(connector))
                .service(list_tags),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/example/repositories/stacker-api/tags?q=latest")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["message"], "OK");
        assert!(body["list"].as_array().unwrap().len() >= 1);
    }
}
