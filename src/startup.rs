use crate::configuration::Settings;
use crate::forms::user::UserForm;
use actix_cors::Cors;
use actix_web::dev::{Server, ServiceRequest};
use actix_web::error::{ErrorInternalServerError, ErrorUnauthorized};
use actix_web::HttpMessage;
use actix_web::{
    web::{self},
    App,
    Error,
    HttpServer,
};
use actix_web_httpauth::{extractors::bearer::BearerAuth, middleware::HttpAuthentication};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

use crate::models::user::User;

#[tracing::instrument(name = "Bearer guard.")]
async fn bearer_guard(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (Error, ServiceRequest)> {
    let settings = req.app_data::<web::Data<Arc<Settings>>>().unwrap();
    let client = reqwest::Client::new();
    let resp = client
        .get(&settings.auth_url)
        .bearer_auth(credentials.token())
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .send()
        .await;

    let resp = match resp {
        Ok(resp) if resp.status().is_success() => resp,
        Ok(resp) => {
            tracing::error!("Authentication service returned no success {:?}", resp);
            // tracing::debug!("{:?}", resp.text().await.unwrap());
            return Err((ErrorUnauthorized("401 Unauthorized"), req));
        }
        Err(err) => {
            tracing::error!("error from reqwest {:?}", err);
            return Err((ErrorInternalServerError(err.to_string()), req));
        }
    };

    let user_form: UserForm = match resp.json().await {
        Ok(user) => {
            tracing::info!("unpacked user {user:?}");
            user
        }
        Err(err) => {
            tracing::error!("can't parse the response body {:?}", err);
            return Err((ErrorUnauthorized(""), req));
        }
    };

    let user: User = match user_form.try_into() // try to convert UserForm into User model
    {
        Ok(user)  => { user }
        Err(err) => {
            tracing::error!("Could not create User from form data: {:?}", err);
            return Err((ErrorUnauthorized("Unauthorized"), req));
        }
    };
    let existent_user = req.extensions_mut().insert(user);
    if existent_user.is_some() {
        tracing::error!("already logged {existent_user:?}");
        return Err((ErrorInternalServerError(""), req));
    }

    Ok(req)
}

pub async fn run(
    listener: TcpListener,
    db_pool: Pool<Postgres>,
    settings: Settings,
) -> Result<Server, std::io::Error> {
    let settings = web::Data::new(Arc::new(settings));
    let db_pool = web::Data::new(db_pool);

    // let address = format!("{}:{}", settings.app_host, settings.app_port);
    // tracing::info!("Start server at {:?}", &address);
    // let listener = std::net::TcpListener::bind(address)
    //     .expect(&format!("failed to bind to {}", settings.app_port));

    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .service(web::scope("/health_check").service(crate::routes::health_check))
            // .service(
            //     web::scope("/client")
            //         .wrap(HttpAuthentication::bearer(bearer_guard))
            //         .wrap(Cors::permissive())
            //         .service(crate::routes::client::add_handler),
            // )
            .service(
                //todo 1. add client_guard. it should fetch client_id and hash from headers. based on db's
                //client secret and input body valiates the input. the client is to be handed over
                //to the http endpoint
                //todo 2. the generation secret and the client bearer to be separated in a separate
                //utils module
                web::scope("/test").service(crate::routes::test::deploy::handler),
            )
            .service(
                web::scope("/rating")
                    .wrap(HttpAuthentication::bearer(bearer_guard))
                    .wrap(Cors::permissive())
                    .service(crate::routes::rating::add_handler)
                    .service(crate::routes::rating::get_handler)
                    .service(crate::routes::rating::list_handler),
            )
            // .service(
            //     web::resource("/stack/{id}")
            //         .route(web::get()
            //             .to(crate::routes::stack::get))
            //         .route(web::post()
            //             .to(crate::routes::stack::update))
            //         .route(web::post()
            //             .to(crate::routes::stack::add)),
            // )
            .service(
                web::scope("/stack")
                    .wrap(HttpAuthentication::bearer(bearer_guard))
                    .wrap(Cors::permissive())
                    .service(crate::routes::stack::deploy::add)
                    .service(crate::routes::stack::add::add)
                    .service(crate::routes::stack::compose::add)
                    .service(crate::routes::stack::compose::admin)
                    .service(crate::routes::stack::get::item)
                    .service(crate::routes::stack::get::list)
            )
            .app_data(db_pool.clone())
            .app_data(settings.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
